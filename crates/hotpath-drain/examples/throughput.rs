//! Sample use of hotpath-drain: several producer threads push one million
//! events in total into a shared registry while a single consumer thread
//! sweeps the queues, then drains what is left at shutdown.
//!
//! Run with:
//!   cargo run -p hotpath-drain --example throughput --release
//!   cargo run -p hotpath-drain --example throughput --release --features hotpath-meta
//!   cargo run -p hotpath-drain --example throughput --release --features hotpath-meta,hotpath-alloc-meta

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use hotpath_drain::{EventProducer, EventQueueRegistry};

const PRODUCERS: usize = 4;
const EVENTS_PER_PRODUCER: u64 = 250_000;
const TOTAL_EVENTS: u64 = PRODUCERS as u64 * EVENTS_PER_PRODUCER;
const SWEEP_INTERVAL: Duration = Duration::from_millis(1);

/// One event; the payload is a `Box` so the consumer's drop and the
/// producer's allocation are both visible under allocation profiling.
struct Event {
    producer: usize,
    seq: u64,
    payload: Box<[u8; 16]>,
}

static REGISTRY: EventQueueRegistry<Event> = EventQueueRegistry::new();

thread_local! {
    static PRODUCER: EventProducer<Event> = REGISTRY.register();
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
fn produce(producer: usize) {
    for seq in 0..EVENTS_PER_PRODUCER {
        if !REGISTRY.is_active() {
            return;
        }
        let event = Event {
            producer,
            seq,
            payload: Box::new([seq as u8; 16]),
        };
        let _ = PRODUCER.try_with(|p| p.push(event));
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
fn consume(batch: &[Event]) -> u64 {
    let mut checksum = 0u64;
    for event in batch {
        checksum = checksum
            .wrapping_add(event.producer as u64)
            .wrapping_add(event.seq)
            .wrapping_add(event.payload[0] as u64);
    }
    checksum
}

/// Sweeps every `SWEEP_INTERVAL` until all producers signal completion, then
/// drains the tails so no event is lost. Returns the total consumed.
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
fn consumer_loop(done: Arc<AtomicBool>) -> u64 {
    let mut batch = Vec::with_capacity(1 << 16);
    let mut consumed = 0u64;
    let mut checksum = 0u64;
    loop {
        REGISTRY.sweep(&mut batch);
        consumed += batch.len() as u64;
        checksum = checksum.wrapping_add(consume(&batch));
        batch.clear();
        if done.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(SWEEP_INTERVAL);
    }
    REGISTRY.drain_all(&mut batch);
    consumed += batch.len() as u64;
    checksum = checksum.wrapping_add(consume(&batch));
    std::hint::black_box(checksum);
    consumed
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::main(percentiles = [50, 95, 99.9]))]
fn main() {
    REGISTRY.set_active(true);
    let start = Instant::now();
    let done = Arc::new(AtomicBool::new(false));

    let consumer = thread::Builder::new()
        .name("consumer".into())
        .spawn({
            let done = Arc::clone(&done);
            move || consumer_loop(done)
        })
        .expect("spawn consumer");

    let producers: Vec<_> = (0..PRODUCERS)
        .map(|id| {
            thread::Builder::new()
                .name(format!("producer-{id}"))
                .spawn(move || produce(id))
                .expect("spawn producer")
        })
        .collect();
    for handle in producers {
        handle.join().expect("producer panicked");
    }

    // Producers have exited, so their thread-local `EventProducer`s are
    // dropped and the queues are marked closed before the final drain.
    REGISTRY.set_active(false);
    done.store(true, Ordering::Release);
    let consumed = consumer.join().expect("consumer panicked");
    let elapsed = start.elapsed();

    assert_eq!(consumed, TOTAL_EVENTS, "events lost in transit");
    println!(
        "{PRODUCERS} producers sent {TOTAL_EVENTS} events, consumer received {consumed} in {elapsed:.2?} ({:.1} M events/s)",
        TOTAL_EVENTS as f64 / elapsed.as_secs_f64() / 1e6
    );
}
