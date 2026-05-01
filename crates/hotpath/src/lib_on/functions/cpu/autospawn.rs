use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

macro_rules! log {
    ($($arg:tt)*) => {{
        eprintln!("[hotpath - cpu autospawn] {}", format_args!($($arg)*));
    }};
}

struct BackendHandle {
    session_dir: PathBuf,
    stop_path: PathBuf,
    profile_path: PathBuf,
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
    let session_id = match session_id() {
        Some(id) => id,
        None => {
            log!("failed to generate CPU profiling session id");
            return;
        }
    };
    let session_dir = PathBuf::from("/tmp/hotpath").join(&session_id);
    if let Err(e) = fs::create_dir_all(&session_dir) {
        log!(
            "failed to create CPU profiling session dir {}: {}",
            session_dir.display(),
            e
        );
        return;
    }
    let stop_path = session_dir.join("stop-profiling");
    let profile_path = session_dir.join("hp.json.gz");

    let _child = match Command::new(&backend_bin)
        .arg("--detach")
        .arg(pid.to_string())
        .arg(&session_dir)
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

    let handle = BackendHandle {
        session_dir,
        stop_path,
        profile_path,
    };
    let slot = HANDLE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(handle);
    }
}

pub(crate) fn stop() -> Option<PathBuf> {
    let handle = HANDLE
        .get()
        .and_then(|m| m.lock().ok().and_then(|mut g| g.take()));
    let Some(handle) = handle else {
        return None;
    };
    if let Err(e) = fs::write(&handle.stop_path, b"") {
        log!(
            "failed to create stop signal {}: {}",
            handle.stop_path.display(),
            e
        );
        return None;
    }

    let t0 = Instant::now();
    let deadline = t0 + Duration::from_secs(15);
    loop {
        if handle.profile_path.exists() {
            return Some(handle.profile_path);
        }

        if Instant::now() >= deadline {
            log!(
                "timed out waiting for CPU profile {} in session {}",
                handle.profile_path.display(),
                handle.session_dir.display()
            );
            return None;
        }

        thread::sleep(Duration::from_millis(100));
    }
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

fn session_id() -> Option<String> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    Some(elapsed.as_millis().to_string())
}
