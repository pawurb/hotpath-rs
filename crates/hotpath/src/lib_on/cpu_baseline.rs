use std::sync::{LazyLock, OnceLock};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender};

const ITERATIONS: u64 = 50_000;

struct CpuBaselineHandle {
    shutdown_tx: Sender<()>,
    completion_rx: Receiver<CpuBaselineResult>,
}

static CPU_BASELINE_OFF: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("HOTPATH_CPU_BASELINE_OFF")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
});

static CPU_BASELINE_HANDLE: OnceLock<CpuBaselineHandle> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub struct CpuBaselineResult {
    pub avg_ns: u64,
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

pub fn init_cpu_baseline() {
    if *CPU_BASELINE_OFF {
        return;
    }

    CPU_BASELINE_HANDLE.get_or_init(|| {
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<CpuBaselineResult>(1);

        std::thread::Builder::new()
            .name("hp-cpu-baseline".into())
            .spawn(move || {
                let mut total_duration = Duration::ZERO;
                let mut count: u128 = 0;

                loop {
                    if shutdown_rx.try_recv().is_ok() {
                        break;
                    }

                    let start = Instant::now();
                    std::hint::black_box(baseline_workload());
                    total_duration += start.elapsed();
                    count += 1;
                }

                if count > 0 {
                    let avg_ns = (total_duration.as_nanos() / count) as u64;
                    let _ = completion_tx.send(CpuBaselineResult { avg_ns });
                }
            })
            .expect("Failed to spawn hp-cpu-baseline thread");

        CpuBaselineHandle {
            shutdown_tx,
            completion_rx,
        }
    });
}

pub fn shutdown_cpu_baseline() -> Option<CpuBaselineResult> {
    let handle = CPU_BASELINE_HANDLE.get()?;
    let _ = handle.shutdown_tx.send(());
    handle.completion_rx.recv().ok()
}
