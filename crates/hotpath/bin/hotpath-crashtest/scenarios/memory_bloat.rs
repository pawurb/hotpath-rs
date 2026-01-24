use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct MemoryBloat {
    activated: Arc<AtomicBool>,
    _handle: JoinHandle<()>,
}

impl MemoryBloat {
    pub fn new() -> Self {
        let activated = Arc::new(AtomicBool::new(false));
        let flag = activated.clone();
        let handle = thread::Builder::new()
            .name("memory-bloat".into())
            .spawn(move || loop {
                if flag.load(Ordering::Relaxed) {
                    allocate(true);
                } else {
                    allocate(false);
                }
                thread::sleep(Duration::from_millis(100));
            })
            .expect("failed to spawn memory-bloat thread");

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
fn allocate(leak: bool) {
    if leak {
        let data: Vec<u8> = vec![1u8; 10 * 1024 * 1024];
        mem::forget(data);
    } else {
        let data: Vec<u8> = vec![1u8; 1024];
        std::hint::black_box(&data);
    }
}
