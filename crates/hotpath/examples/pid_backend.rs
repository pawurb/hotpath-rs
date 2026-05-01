use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

fn main() {
    if let Err(err) = run() {
        eprintln!("pid_backend error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mode = match args.next() {
        Some(mode) => mode,
        None => return Err(usage()),
    };

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

    let output_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("hp.json.gz"));
    let current_exe =
        env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;

    let child = Command::new(&current_exe)
        .arg("--worker")
        .arg(pid.to_string())
        .arg(&output_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn detached worker {}: {e}", current_exe.display()))?;

    let _ = child.id();
    Ok(())
}

fn run_worker(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let pid = args
        .next()
        .ok_or_else(usage)?
        .parse::<u32>()
        .map_err(|e| format!("invalid pid: {e}"))?;

    let output_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("hp.json.gz"));
    let samply_bin =
        env::var("HOTPATH_CPU_SAMPLY_BIN").unwrap_or_else(|_| "samply".to_string());

    thread::sleep(Duration::from_secs(3));

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
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn {samply_bin}: {e}"))?;

    thread::sleep(Duration::from_secs(5));

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
    "usage: cargo run -p hotpath --example pid_backend -- (--detach|--worker) <pid> [output_path]"
        .to_string()
}
