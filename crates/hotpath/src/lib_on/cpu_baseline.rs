use std::sync::{LazyLock, OnceLock};

use crossbeam_channel::{bounded, Receiver, Sender};

const ITERATIONS: u64 = 50_000;

struct CpuBaselineHandle {
    shutdown_tx: Sender<()>,
    completion_rx: Receiver<CpuBaselineResult>,
}

static CPU_BASELINE_OFF: LazyLock<bool> =
    LazyLock::new(|| crate::shared::env_flag("HOTPATH_CPU_BASELINE_OFF"));

static CPU_BASELINE_HANDLE: OnceLock<CpuBaselineHandle> = OnceLock::new();

#[cfg(unix)]
fn thread_cpu_time_ns() -> Option<u64> {
    let mut ts = std::mem::MaybeUninit::<libc::timespec>::uninit();
    let ret = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, ts.as_mut_ptr()) };
    if ret != 0 {
        return None;
    }
    let ts = unsafe { ts.assume_init() };
    Some(ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64)
}

#[cfg(windows)]
fn thread_cpu_time_ns() -> Option<u64> {
    Some(0)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CpuBaselineResult {
    pub(crate) avg_ns: u64,
}

#[inline(never)]
fn baseline_workload() -> u64 {
    let mut x: u64 = 0xcbf29ce484222325;
    for _ in 0..ITERATIONS {
        x = std::hint::black_box(x.wrapping_mul(0x100000001b3));
        x = std::hint::black_box(x ^ (x >> 17));
    }
    x
}

pub(crate) fn init_cpu_baseline() {
    if *CPU_BASELINE_OFF {
        return;
    }

    CPU_BASELINE_HANDLE.get_or_init(|| {
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<CpuBaselineResult>(1);

        std::thread::Builder::new()
            .name("hp-cpu-baseline".into())
            .spawn(move || {
                let _suspend = crate::lib_on::SuspendAllocTracking::new();
                let mut total_ns: u128 = 0;
                let mut count: u128 = 0;
                let sample_interval = std::time::Duration::from_millis(50);

                loop {
                    let Some(start) = thread_cpu_time_ns() else {
                        continue;
                    };
                    std::hint::black_box(baseline_workload());
                    let Some(end) = thread_cpu_time_ns() else {
                        continue;
                    };

                    total_ns += (end - start) as u128;
                    count += 1;

                    match shutdown_rx.recv_timeout(sample_interval) {
                        Ok(()) => break,
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    }
                }

                if let Some(avg_ns) = total_ns.checked_div(count) {
                    let _ = completion_tx.send(CpuBaselineResult {
                        avg_ns: avg_ns as u64,
                    });
                }
            })
            .expect("Failed to spawn hp-cpu-baseline thread");

        CpuBaselineHandle {
            shutdown_tx,
            completion_rx,
        }
    });
}

pub(crate) fn shutdown_cpu_baseline() -> Option<CpuBaselineResult> {
    let handle = CPU_BASELINE_HANDLE.get()?;
    let _ = handle.shutdown_tx.send(());
    handle.completion_rx.recv().ok()
}
