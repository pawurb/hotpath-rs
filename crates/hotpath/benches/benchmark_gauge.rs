use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::thread;

fn bench_gauge_single_thread(c: &mut Criterion) {
    // Warmup the debug system
    hotpath::gauge!("warmup").set(0u64);
    thread::sleep(std::time::Duration::from_millis(10));

    c.bench_function("gauge_set_single_thread", |b| {
        let mut i = 0u64;
        b.iter(|| {
            hotpath::gauge!("bench_single").set(black_box(i));
            i = i.wrapping_add(1);
        });
    });
}

fn bench_gauge_multi_thread(c: &mut Criterion) {
    let num_threads = 4usize;

    c.bench_function("gauge_set_4_threads", |b| {
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            let handles: Vec<_> = (0..num_threads)
                .map(|t| {
                    thread::spawn(move || {
                        for i in 0..iters {
                            hotpath::gauge!("bench_multi").set(black_box(i + t as u64));
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }
            start.elapsed()
        });
    });
}

criterion_group!(benches, bench_gauge_single_thread, bench_gauge_multi_thread);
criterion_main!(benches);
