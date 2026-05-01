#[path = "../dev_logging.rs"]
mod dev_logging;

use std::env;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(feature = "dev")]
use tracing::{error, info, warn};

#[cfg(not(feature = "dev"))]
macro_rules! noop_log {
    ($($tt:tt)*) => {{
        let _ = format_args!($($tt)*);
    }};
}
#[cfg(not(feature = "dev"))]
use noop_log as error;
#[cfg(not(feature = "dev"))]
use noop_log as info;
#[cfg(not(feature = "dev"))]
use noop_log as warn;

fn main() {
    dev_logging::init_logging();

    if let Err(err) = run() {
        error!("hotpath-pid-backend failed: {err}");
        eprintln!("hotpath-pid-backend error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mode = match args.next() {
        Some(mode) => mode,
        None => return Err(usage()),
    };
    info!("hotpath-pid-backend mode={mode}");

    if mode == "--detach" {
        return detach_worker(args);
    }

    if mode != "--worker" {
        return Err(usage());
    }

    run_worker(args)
}

fn detach_worker(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let pid = args
        .next()
        .ok_or_else(usage)?
        .parse::<u32>()
        .map_err(|e| format!("invalid pid: {e}"))?;

    let session_dir = args.next().map(PathBuf::from).ok_or_else(usage)?;
    let current_exe =
        env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;
    info!(
        "detach requested pid={} session_dir={} exe={}",
        pid,
        session_dir.display(),
        current_exe.display()
    );

    let worker_stdout =
        open_log_file().map_err(|e| format!("failed to open worker stdout log: {e}"))?;
    let worker_stderr =
        open_log_file().map_err(|e| format!("failed to open worker stderr log: {e}"))?;

    let child = Command::new(&current_exe)
        .arg("--worker")
        .arg(pid.to_string())
        .arg(&session_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(worker_stdout))
        .stderr(Stdio::from(worker_stderr))
        .spawn()
        .map_err(|e| {
            format!(
                "failed to spawn detached worker {}: {e}",
                current_exe.display()
            )
        })?;
    info!("detached worker pid={} for target pid={}", child.id(), pid);

    Ok(())
}

fn run_worker(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let pid = args
        .next()
        .ok_or_else(usage)?
        .parse::<u32>()
        .map_err(|e| format!("invalid pid: {e}"))?;

    let session_dir = args.next().map(PathBuf::from).ok_or_else(usage)?;
    let output_path = session_dir.join("hp.json.gz");
    let stop_path = session_dir.join("stop-profiling");
    let samply_bin = env::var("HOTPATH_CPU_SAMPLY_BIN").unwrap_or_else(|_| "samply".to_string());
    info!(
        "worker starting target_pid={} session_dir={} output={} samply_bin={}",
        pid,
        session_dir.display(),
        output_path.display(),
        samply_bin
    );
    info!("profile output path reserved at {}", output_path.display());

    thread::sleep(Duration::from_secs(3));
    info!("worker delay complete, launching samply for pid={}", pid);

    let samply_stdout =
        open_log_file().map_err(|e| format!("failed to open samply stdout log: {e}"))?;
    let samply_stderr =
        open_log_file().map_err(|e| format!("failed to open samply stderr log: {e}"))?;

    let mut child = Command::new(&samply_bin)
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
        .stdout(Stdio::from(samply_stdout))
        .stderr(Stdio::from(samply_stderr))
        .spawn()
        .map_err(|e| format!("failed to spawn {samply_bin}: {e}"))?;
    info!(
        "samply child pid={} attached to target pid={}",
        child.id(),
        pid
    );

    let poll_started_at = Instant::now();
    loop {
        if stop_path.exists() {
            info!(
                "stop signal observed at {} after {:?}",
                stop_path.display(),
                poll_started_at.elapsed()
            );
            break;
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                info!(
                    "samply exited before stop signal status={} output={}",
                    status,
                    output_path.display()
                );
                if !status.success() {
                    return Err(format!(
                        "samply exited with status {} while producing {}",
                        status,
                        output_path.display()
                    ));
                }
                return Ok(());
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                return Err(format!("failed to poll samply child {}: {e}", child.id()));
            }
        }
    }
    info!("sending SIGINT to samply pid={}", child.id());

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
        info!("SIGINT delivered to samply pid={}", child.id());
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
                warn!("SIGKILL sent to samply pid={}", child.id());
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
    info!(
        "samply exited status={} output={} target_pid={}",
        status,
        output_path.display(),
        pid
    );
    match std::fs::metadata(&output_path) {
        Ok(metadata) => info!(
            "profile file created path={} size={} bytes",
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

    Ok(())
}

fn usage() -> String {
    "usage: hotpath-pid-backend (--detach|--worker) <pid> <session_dir>".to_string()
}

fn open_log_file() -> std::io::Result<std::fs::File> {
    std::fs::create_dir_all("log")?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open("log/development.log")
}
