use std::time::Duration;

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn example_function() {
    std::thread::sleep(Duration::from_millis(10));
}

#[cfg_attr(feature = "hotpath", hotpath::main(percentiles = [75, 95], format = "json-pretty"))]
fn main() {
    for _ in 0..5 {
        example_function();
    }
}
