use hotpath::dev_logging::{error, info, warn};

use std::env;
#[cfg(feature = "dev")]
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::thread;
use std::time::Duration;

static SAMPLY_BIN: LazyLock<String> =
    LazyLock::new(|| env::var("HOTPATH_SAMPLY_BIN").unwrap_or_else(|_| "samply".to_string()));

fn main() {
    hotpath::dev_logging::init_logging();

    if let Err(err) = run() {
        error!("hotpath-samply failed: {err}");
        eprintln!("hotpath-samply error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mode = args.next().ok_or("missing mode argument")?;

    match mode.as_str() {
        "--detach" => detach_worker(args),
        "--worker" => run_worker(args),
        other => Err(format!("unknown mode: {other}")),
    }
}

fn detach_worker(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let pid = args
        .next()
        .ok_or("missing pid argument")?
        .parse::<u32>()
        .map_err(|e| format!("invalid pid: {e}"))?;

    let session_dir = args
        .next()
        .map(PathBuf::from)
        .ok_or("missing session_dir argument")?;
    let current_exe =
        env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;

    let worker_stdout =
        child_stdio().map_err(|e| format!("failed to open worker stdout log: {e}"))?;
    let worker_stderr =
        child_stdio().map_err(|e| format!("failed to open worker stderr log: {e}"))?;

    Command::new(&current_exe)
        .arg("--worker")
        .arg(pid.to_string())
        .arg(&session_dir)
        .stdin(Stdio::null())
        .stdout(worker_stdout)
        .stderr(worker_stderr)
        .spawn()
        .map_err(|e| {
            format!(
                "failed to spawn detached worker {}: {e}",
                current_exe.display()
            )
        })?;

    Ok(())
}

fn run_worker(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let pid = args
        .next()
        .ok_or("missing pid argument")?
        .parse::<u32>()
        .map_err(|e| format!("invalid pid: {e}"))?;

    let session_dir = args
        .next()
        .map(PathBuf::from)
        .ok_or("missing session_dir argument")?;
    let output_path = session_dir.join("hp.json.gz");
    let stop_path = session_dir.join("stop-profiling");
    let done_path = session_dir.join("done");

    let samply_stdout =
        child_stdio().map_err(|e| format!("failed to open samply stdout log: {e}"))?;
    let samply_stderr =
        child_stdio().map_err(|e| format!("failed to open samply stderr log: {e}"))?;

    info!(
        "worker starting target_pid={} output={} samply_bin={}",
        pid,
        output_path.display(),
        *SAMPLY_BIN
    );

    let mut child = Command::new(&*SAMPLY_BIN)
        .args([
            "record",
            "--pid",
            &pid.to_string(),
            "--save-only",
            "-o",
            output_path
                .to_str()
                .ok_or_else(|| format!("non-utf8 output path: {}", output_path.display()))?,
        ])
        .stdin(Stdio::null())
        .stdout(samply_stdout)
        .stderr(samply_stderr)
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", *SAMPLY_BIN))?;

    loop {
        if stop_path.exists() {
            break;
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(format!(
                        "samply exited with status {} while producing {}",
                        status,
                        output_path.display()
                    ));
                }
                signal_done(&done_path);
                return Ok(());
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                return Err(format!("failed to poll samply child {}: {e}", child.id()));
            }
        }
    }
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .map_err(|e| format!("failed to send SIGINT to samply child {}: {e}", child.id()))?;
        if !status.success() {
            return Err(format!(
                "kill -INT failed for samply child {} with status {}",
                child.id(),
                status
            ));
        }
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() >= deadline => {
                warn!(
                    "samply pid={} did not exit after SIGINT, sending SIGKILL",
                    child.id()
                );
                let _ = child.kill();
                break child
                    .wait()
                    .map_err(|e| format!("failed to wait for samply child {}: {e}", child.id()))?;
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                return Err(format!(
                    "failed to wait for samply child {}: {e}",
                    child.id()
                ));
            }
        }
    };
    match std::fs::metadata(&output_path) {
        Ok(metadata) => info!(
            "profile written path={} size={} bytes",
            output_path.display(),
            metadata.len()
        ),
        Err(err) => warn!(
            "profile file missing after samply exit path={} error={}",
            output_path.display(),
            err
        ),
    }
    if !status.success() {
        return Err(format!(
            "samply exited with status {} while producing {}",
            status,
            output_path.display()
        ));
    }

    signal_done(&done_path);
    Ok(())
}

fn signal_done(done_path: &std::path::Path) {
    if let Err(e) = std::fs::write(done_path, b"") {
        warn!(
            "failed to write done sentinel {}: {}",
            done_path.display(),
            e
        );
    }
}

#[cfg(feature = "dev")]
fn child_stdio() -> std::io::Result<Stdio> {
    let path = &*hotpath::dev_logging::DEV_LOG_PATH;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    Ok(Stdio::from(file))
}

#[cfg(not(feature = "dev"))]
fn child_stdio() -> std::io::Result<Stdio> {
    Ok(Stdio::null())
}
