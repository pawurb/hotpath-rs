use num_bigint::BigUint;

/// Iterative fibonacci using BigUint - no overflow
#[cfg_attr(feature = "hotpath", hotpath::measure(log = true))]
fn fibonacci(n: u64) -> BigUint {
    if n == 0 {
        return BigUint::from(0u32);
    }
    if n == 1 {
        return BigUint::from(1u32);
    }

    let mut a = BigUint::from(0u32);
    let mut b = BigUint::from(1u32);

    for _ in 2..=n {
        let next = &a + &b;
        a = b;
        b = next;
    }

    b
}

#[cfg_attr(feature = "hotpath", hotpath::main)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Fibonacci computation with hotpath profiling");
    println!("Run TUI in another terminal with:");
    println!("  cargo run --bin hotpath --features tui -- console --metrics-port 6870");
    println!();

    // Compute increasingly larger fibonacci values
    // Each call takes progressively longer, visible in TUI logs
    let mut n = 100_000u64;

    loop {
        let result = fibonacci(n);
        let digits = result.to_string().len();
        println!("fibonacci({}) has {} digits", n, digits);

        // Increase by 10% each time so timing grows noticeably
        n = n + n / 10;
    }
}
