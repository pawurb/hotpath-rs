use std::time::Duration;

#[hotpath::measure]
fn example_function() {
    std::thread::sleep(Duration::from_millis(10));
}

#[hotpath::main(format = "json", output_path = "tmp/functions_output_test.json")]
fn main() {
    for _ in 0..5 {
        example_function();
    }
}
