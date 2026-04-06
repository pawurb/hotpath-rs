use std::time::Duration;

#[hotpath::measure]
fn example_function() {
    std::thread::sleep(Duration::from_millis(10));
}

#[hotpath::main(percentiles = [50, 90, 99.9])]
fn main() {
    for _ in 0..5 {
        example_function();
    }
}
