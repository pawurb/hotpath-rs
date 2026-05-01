use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

macro_rules! log {
    ($($arg:tt)*) => {{
        eprintln!("[hotpath - cpu autospawn] {}", format_args!($($arg)*));
    }};
}

struct SamplyHandle {
    child: Child,
}

static HANDLE: OnceLock<Mutex<Option<SamplyHandle>>> = OnceLock::new();
static PROFILE_PATH: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn profile_path() -> Option<&'static Path> {
    PROFILE_PATH.get().map(|p| p.as_path())
}

pub(crate) fn start() {
    log!("autospawn::start() entered");

    #[cfg(not(unix))]
    {
        log!("samply autospawn unsupported on this platform");
        return;
    }

    #[cfg(unix)]
    {
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("hotpath-cpu-{pid}.json.gz"));
        let stderr_path = std::env::temp_dir().join(format!("hotpath-cpu-{pid}.stderr.log"));
        log!(
            "autospawn about to spawn samply pid={pid} bin={} output={} stderr_log={}",
            samply_bin(),
            path.display(),
            stderr_path.display()
        );

        let stderr_file = match std::fs::File::create(&stderr_path) {
            Ok(f) => f,
            Err(e) => {
                log!("failed to create samply stderr log: {e}");
                return;
            }
        };

        let mut cmd = Command::new(samply_bin());
        cmd.args([
            "record",
            "--pid",
            &pid.to_string(),
            "--save-only",
            "-o",
            path.to_str().unwrap_or(""),
        ])
        .args(rate_args())
        .args(extra_args())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file));

        // Detach from our session so samply (or any sudo it spawns) cannot
        // read from /dev/tty and block the parent.
        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let t0 = Instant::now();
        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                log!(
                    "samply autospawn spawn failed: {e} \
                     (install: cargo install samply, or set HOTPATH_CPU_SAMPLY_BIN)"
                );
                return;
            }
        };
        log!(
            "autospawn samply spawned child_pid={} (took {:?})",
            child.id(),
            t0.elapsed()
        );

        log!("autospawn start() done, profile path={}", path.display());

        let _ = PROFILE_PATH.set(path);
        let _ = HANDLE.set(Mutex::new(Some(SamplyHandle { child })));
    }
}

pub(crate) fn stop() {
    log!("autospawn::stop() entered");
    let handle = HANDLE
        .get()
        .and_then(|m| m.lock().ok().and_then(|mut g| g.take()));
    let Some(mut h) = handle else {
        log!("autospawn stop() no handle, returning");
        return;
    };
    log!("autospawn sending SIGINT to samply pid={}", h.child.id());

    #[cfg(unix)]
    unsafe {
        libc::kill(h.child.id() as i32, libc::SIGINT);
    }

    let t0 = Instant::now();
    let deadline = t0 + Duration::from_secs(5);
    loop {
        match h.child.try_wait() {
            Ok(Some(status)) => {
                log!("autospawn samply exited: {status} after {:?}", t0.elapsed());
                break;
            }
            Ok(None) if Instant::now() >= deadline => {
                log!("autospawn samply did not exit after 5s SIGINT, killing");
                let _ = h.child.kill();
                let _ = h.child.wait();
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                log!("autospawn wait error: {e}");
                break;
            }
        }
    }
    log!("autospawn flushing 100ms");
    thread::sleep(Duration::from_millis(100));
    log!("autospawn stop() done");
}

fn samply_bin() -> String {
    std::env::var("HOTPATH_CPU_SAMPLY_BIN").unwrap_or_else(|_| "samply".to_string())
}

fn rate_args() -> Vec<String> {
    match std::env::var("HOTPATH_CPU_SAMPLY_RATE") {
        Ok(rate) if !rate.is_empty() => vec!["--rate".to_string(), rate],
        _ => Vec::new(),
    }
}

fn extra_args() -> Vec<String> {
    std::env::var("HOTPATH_CPU_SAMPLY_ARGS")
        .unwrap_or_default()
        .split_whitespace()
        .map(String::from)
        .collect()
}
