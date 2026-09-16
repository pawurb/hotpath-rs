//! Minimal scenario for miri: several producer threads push boxed events
//! across chunk boundaries while the consumer sweeps concurrently, then the
//! remainder is drained after the producers exit.
//!
//! Run with:
//!   cargo +nightly miri test -p hotpath-drain --test miri

use std::thread;

use hotpath_drain::{EventProducer, EventQueueRegistry, CHUNK_SIZE};

static REGISTRY: EventQueueRegistry<Box<u64>> = EventQueueRegistry::new();

thread_local! {
    static PRODUCER: EventProducer<Box<u64>> = REGISTRY.register();
}

#[test]
fn push_sweep_drain_across_threads() {
    const PRODUCERS: u64 = 3;
    const PER_PRODUCER: u64 = CHUNK_SIZE as u64 * 2 + 5;

    REGISTRY.set_active(true);
    let handles: Vec<_> = (0..PRODUCERS)
        .map(|producer| {
            thread::spawn(move || {
                for seq in 0..PER_PRODUCER {
                    PRODUCER.with(|p| p.push(Box::new(producer * PER_PRODUCER + seq)));
                }
            })
        })
        .collect();

    let mut out = Vec::new();
    for _ in 0..4 {
        REGISTRY.sweep(&mut out);
        thread::yield_now();
    }
    for handle in handles {
        handle.join().unwrap();
    }
    REGISTRY.set_active(false);
    REGISTRY.drain_all(&mut out);

    let mut values: Vec<u64> = out.into_iter().map(|b| *b).collect();
    values.sort_unstable();
    let expected: Vec<u64> = (0..PRODUCERS * PER_PRODUCER).collect();
    assert_eq!(values, expected);
}
