# Function performance profiling: timing and memory metrics

To start profiling functions performance you'll only need `#[hotpath::main]` and `#[hotpath::measure]` macros:

```rust
#[hotpath::measure]
fn sync_function(sleep: u64) {
    std::thread::sleep(Duration::from_nanos(sleep));
}

#[hotpath::measure]
async fn async_function(sleep: u64) {
    tokio::time::sleep(Duration::from_nanos(sleep)).await;
}

// When using with tokio, place the #[tokio::main] first
#[tokio::main]
#[hotpath::main]
async fn main() {
    for i in 0..100 {
        // Measured functions will automatically send metrics
        sync_function(i);
        async_function(i * 2).await;

        // Measure code blocks with static labels
        hotpath::measure_block!("custom_block", {
            std::thread::sleep(Duration::from_nanos(i * 3))
        });
    }
}
```

When the `hotpath` feature is disabled, all macros are noop and have zero compile or runtime overhead.

Run your program with a hotpath feature:

```bash
cargo run --features=hotpath
```

Output:

```text
[hotpath] Performance summary from basic::main (Total time: 122.13ms):
+-----------------------+-------+---------+---------+----------+---------+
| Function              | Calls | Avg     | P99     | Total    | % Total |
+-----------------------+-------+---------+---------+----------+---------+
| basic::async_function | 100   | 1.16ms  | 1.20ms  | 116.03ms | 95.01%  |
+-----------------------+-------+---------+---------+----------+---------+
| custom_block          | 100   | 17.09µs | 39.55µs | 1.71ms   | 1.40%   |
+-----------------------+-------+---------+---------+----------+---------+
| basic::sync_function  | 100   | 16.99µs | 35.42µs | 1.70ms   | 1.39%   |
+-----------------------+-------+---------+---------+----------+---------+
```

## `#[hotpath::main]` macro

Attribute macro that initializes the background measurement processing when applied. Supports parameters:

- `percentiles = [50, 95, 99]` - Custom percentiles to display (defaults to `[95]`)
- `format = "json"` - Output format `"table"`, `"json"`, `"json-pretty"`, (defaults to `table`)
- `limit = 20` - Maximum number of functions to display (default: `15`, `0` = show all)
- `timeout = 5000` - Optional timeout in milliseconds. If specified, the program will print the report and exit after the timeout. Useful for profiling long-running programs like HTTP servers, if you don't want to use live monitoring TUI dashboard.

## `#[hotpath::measure]` macro

An attribute macro that instruments functions to send timing/memory measurements to the background processor. Parameters:

- `log = true` - logs the result value when the function returns (requires `std::fmt::Debug` on return type)

Example:

```rust
#[hotpath::measure(log = true)]
fn compute() -> i32 {
    // The result value will be logged in TUI console
    42
}
```

<img loading="lazy" src="{{#asset-hash images/functions-log.png}}" alt="hotpath-rs TUI showing function return value logging">

## `#[hotpath::measure_all]` macro

An attribute macro that applies `#[measure]` to all functions in a `mod` or `impl` block. Useful for bulk instrumentation without annotating each function individually. Can be used on:

- **Inline module declarations** - Instruments all functions within the module
- **Impl blocks** - Instruments all methods in the implementation

Example:

```rust
// Measure all methods in an impl block
#[hotpath::measure_all]
impl Calculator {
    fn add(&self, a: u64, b: u64) -> u64 { a + b }
    fn multiply(&self, a: u64, b: u64) -> u64 { a * b }
    async fn async_compute(&self) -> u64 { /* ... */ }
}

// Measure all functions in a module
#[hotpath::measure_all]
mod math_operations {
    pub fn complex_calculation(x: f64) -> f64 { /* ... */ }
    pub async fn fetch_data() -> Vec<u8> { /* ... */ }
}
```

> **Note:** Once Rust stabilizes [`#![feature(proc_macro_hygiene)]`](https://doc.rust-lang.org/beta/unstable-book/language-features/proc-macro-hygiene.html?highlight=proc_macro_hygiene#proc_macro_hygiene) and [`#![feature(custom_inner_attributes)]`](https://doc.rust-lang.org/beta/unstable-book/language-features/custom-inner-attributes.html), it will be possible to use `#![measure_all]` as an inner attribute directly inside module files (e.g., at the top of `math_operations.rs`) to automatically instrument all functions in that module.

## `#[hotpath::skip]` macro

A marker attribute that excludes specific functions from instrumentation when used within a module or impl block annotated with `#[measure_all]`. The function executes normally but doesn't send measurements to the profiling system.

Example:

```rust
#[hotpath::measure_all]
mod operations {
    pub fn important_function() { /* ... */ } // Measured

    #[hotpath::skip]
    pub fn not_so_important_function() { /* ... */ } // NOT measured
}
```

## `hotpath::measure_block!` macro

Macro that measures the execution time of a code block with a static string label.

```rust
#[hotpath::main]
fn main() {
    for i in 0..100 {
        // Measure code blocks with static labels
        hotpath::measure_block!("custom_block", {
            std::thread::sleep(Duration::from_nanos(i * 3))
        });
    }
}
```

If `hotpath` feature is disabled, the code inside block will still execute.

## Memory and allocations profiling

In addition to time-based profiling, `hotpath` can track memory allocations. This feature uses a custom global allocator from [allocation-counter crate](https://github.com/fornwall/allocation-counter) to intercept all memory allocations and provides detailed statistics about memory usage per function.


Run your program with the allocation tracking feature to print a similar report:

```
cargo run --features='hotpath,hotpath-alloc'
```

<img loading="lazy" src="{{#asset-hash images/hotpath-alloc-report.png}}" alt="hotpath-rs memory allocation profiling report showing per-function byte counts">

### Async functions

Allocation profiling uses thread-local storage to track memory usage. Since async tasks can migrate between threads, allocation counts for async functions would be inaccurate and it currently not supported. 

## Memory profiling modes

By default, allocation tracking is **cumulative**, meaning that a function's allocation count includes all allocations made by functions it calls (nested calls). Notably, it produces invalid results for recursive functions. To track only **exclusive** allocations (direct allocations made by each function, excluding nested calls), set the `HOTPATH_ALLOC_SELF=true` environment variable when running your program.

## Nightly features

When Rust stabilizes [`#![feature(proc_macro_hygiene)]`](https://doc.rust-lang.org/beta/unstable-book/language-features/proc-macro-hygiene.html?highlight=proc_macro_hygiene#proc_macro_hygiene) and [`#![feature(custom_inner_attributes)]`](https://doc.rust-lang.org/beta/unstable-book/language-features/custom-inner-attributes.html), it will be possible to use `#![measure_all]` as an inner attribute directly inside module files (e.g., at the top of `math_operations.rs`) to automatically instrument all functions in that module.
