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

    eprintln!(
        "[pid_backend] starting: bin={} pid={} output={} stderr=stdout",
        samply_bin,
        pid,
        output_path.display()
    );

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
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn {samply_bin}: {e}"))?;

    eprintln!("[pid_backend] samply child pid={}", child.id());
    thread::sleep(Duration::from_secs(5));
    eprintln!("[pid_backend] stopping samply after 5s with SIGINT");

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
                eprintln!("[pid_backend] samply did not exit after SIGINT, sending SIGKILL");
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

    eprintln!(
        "[pid_backend] samply exited status={} output={} stderr=stdout",
        status,
        output_path.display()
    );

    Ok(())
}

fn usage() -> String {
    "usage: cargo run -p hotpath --example pid_backend -- <pid> [output_path]".to_string()
}
