use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::dev_logging::warn;

struct BackendHandle {
    session_id: String,
    session_dir: PathBuf,
    stop_path: PathBuf,
    profile_path: PathBuf,
    done_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionInfo {
    pub(crate) session_id: String,
    pub(crate) session_dir: PathBuf,
}

static HANDLE: OnceLock<Mutex<Option<BackendHandle>>> = OnceLock::new();

static BACKEND_BIN: LazyLock<PathBuf> = LazyLock::new(|| {
    std::env::var("HOTPATH_SAMPLY_WRAPPER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(format!("hotpath-samply{}", std::env::consts::EXE_SUFFIX))
        })
});

pub(crate) fn start() {
    let pid = std::process::id();
    let backend_bin = &*BACKEND_BIN;
    let session_id = match session_id() {
        Some(id) => id,
        None => {
            warn!("failed to generate CPU profiling session id");
            return;
        }
    };
    let session_dir = PathBuf::from("/tmp/hotpath").join(&session_id);
    if let Err(e) = fs::create_dir_all(&session_dir) {
        warn!(
            "failed to create CPU profiling session dir {}: {}",
            session_dir.display(),
            e
        );
        return;
    }
    let stop_path = session_dir.join("stop-profiling");
    let profile_path = session_dir.join("hp.json.gz");
    let done_path = session_dir.join("done");

    let _child = match Command::new(backend_bin)
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
            warn!(
                "failed to spawn backend process via {}: {}",
                backend_bin.display(),
                e
            );
            return;
        }
    };

    let handle = BackendHandle {
        session_id,
        session_dir,
        stop_path,
        profile_path,
        done_path,
    };
    let slot = HANDLE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(handle);
    }
}

pub(crate) fn current_session() -> Option<SessionInfo> {
    let slot = HANDLE.get()?;
    let guard = slot.lock().ok()?;
    guard.as_ref().map(|h| SessionInfo {
        session_id: h.session_id.clone(),
        session_dir: h.session_dir.clone(),
    })
}

pub(crate) fn stop() -> Option<PathBuf> {
    let handle = HANDLE
        .get()
        .and_then(|m| m.lock().ok().and_then(|mut g| g.take()));
    let handle = handle?;
    if let Err(e) = fs::write(&handle.stop_path, b"") {
        warn!(
            "failed to create stop signal {}: {}",
            handle.stop_path.display(),
            e
        );
        return None;
    }

    let t0 = Instant::now();
    let deadline = t0 + Duration::from_secs(15);
    loop {
        if handle.done_path.exists() {
            return Some(handle.profile_path);
        }

        if Instant::now() >= deadline {
            warn!(
                "timed out waiting for CPU profile done sentinel {} in session {}",
                handle.done_path.display(),
                handle.session_dir.display()
            );
            return None;
        }

        thread::sleep(Duration::from_millis(100));
    }
}

fn session_id() -> Option<String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX_EPOCH");
    Some(format!("{}-{}", std::process::id(), elapsed.as_nanos()))
}
