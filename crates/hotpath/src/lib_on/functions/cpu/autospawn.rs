use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

macro_rules! log {
    ($($arg:tt)*) => {{
        eprintln!("[hotpath - cpu autospawn] {}", format_args!($($arg)*));
    }};
}

struct BackendHandle {
    child: Child,
}

static HANDLE: OnceLock<Mutex<Option<BackendHandle>>> = OnceLock::new();

pub(crate) fn start() {
    let pid = std::process::id();
    let backend_bin = match backend_bin() {
        Some(path) => path,
        None => {
            log!("failed to resolve hotpath-pid-backend binary path");
            return;
        }
    };

    let child = match Command::new(&backend_bin)
        .arg("--detach")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            log!(
                "failed to spawn backend process via {}: {}",
                backend_bin.display(),
                e
            );
            return;
        }
    };

    let _ = HANDLE.set(Mutex::new(Some(BackendHandle { child })));
}

pub(crate) fn stop() {
    let handle = HANDLE
        .get()
        .and_then(|m| m.lock().ok().and_then(|mut g| g.take()));
    let Some(mut handle) = handle else {
        return;
    };

    let t0 = Instant::now();
    let deadline = t0 + Duration::from_secs(15);
    loop {
        match handle.child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = handle.child.kill();
                let _ = handle.child.wait();
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(_e) => break,
        }
    }

    thread::sleep(Duration::from_millis(100));
}

fn backend_bin() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("HOTPATH_CPU_BACKEND_BIN") {
        if !path.is_empty() {
            return Some(std::path::PathBuf::from(path));
        }
    }

    let current_exe = std::env::current_exe().ok()?;
    let parent = current_exe.parent()?;
    Some(parent.join(format!(
        "hotpath-pid-backend{}",
        std::env::consts::EXE_SUFFIX
    )))
}
