use std::time::Duration;

fn main() {
    hotpath::HotpathGuardBuilder::new("guard_timeout_threads")
        .with_sections(vec![hotpath::Section::Threads])
        .build_with_timeout(Duration::from_secs(2));

    loop {
        std::hint::black_box(0u64.wrapping_add(1));
        std::thread::sleep(Duration::from_millis(10));
    }
}
