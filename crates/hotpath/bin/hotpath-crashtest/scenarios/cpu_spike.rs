use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct CpuSpike {
    activated: Arc<AtomicBool>,
    _handle: JoinHandle<()>,
}

impl CpuSpike {
    pub fn new() -> Self {
        let activated = Arc::new(AtomicBool::new(false));
        let flag = activated.clone();
        let handle = thread::Builder::new()
            .name("cpu-spike".into())
            .spawn(move || loop {
                if flag.load(Ordering::Relaxed) {
                    heavy_work();
                } else {
                    light_work();
                    thread::sleep(Duration::from_millis(100));
                }
            })
            .expect("failed to spawn cpu-spike thread");

        Self {
            activated,
            _handle: handle,
        }
    }

    pub fn set_activated(&mut self, activated: bool) {
        self.activated.store(activated, Ordering::Relaxed);
    }

    pub fn is_activated(&self) -> bool {
        self.activated.load(Ordering::Relaxed)
    }
}

#[hotpath::measure]
fn heavy_work() {
    let mut x: u64 = 0;
    for _ in 0..10_000_000 {
        x = x.wrapping_add(1);
    }
    std::hint::black_box(x);
}

#[hotpath::measure]
fn light_work() {
    let mut x: u64 = 0;
    for _ in 0..500_000 {
        x = x.wrapping_add(1);
    }
    std::hint::black_box(x);
}
