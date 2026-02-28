use proc_macro::TokenStream;

#[cfg(feature = "hotpath")]
mod lib_on;

#[cfg(not(feature = "hotpath"))]
mod lib_off;

/// Initializes the hotpath profiling system and generates a performance report on program exit.
///
/// This attribute macro should be applied to your program's main (or other entry point) function
/// to enable profiling. It creates a guard that initializes the background measurement processing
/// thread and automatically displays a performance summary when the program exits. Additionally
/// it creates a measurement guard that will be used to measure the wrapper function itself.
///
/// For programmatic control over the same options, see
/// [`HotpathGuardBuilder`](../hotpath/struct.HotpathGuardBuilder.html).
///
/// # Parameters
///
/// * `percentiles` - Array of percentile values (0-100) to compute. Default: `[95]`
/// * `format` - Output format: `"table"` (default), `"json"`, `"json-pretty"`, or `"none"`
/// * `limit` - Maximum number of functions in the report (0 = unlimited). Default: `15`
/// * `output_path` - File path for the report. Defaults to stdout. Overridden by `HOTPATH_OUTPUT_PATH` env var.
/// * `report` - Comma-separated sections to include: `"functions-timing"`, `"functions-alloc"`, `"channels"`, `"streams"`, `"futures"`, `"threads"`, or `"all"`. Overridden by `HOTPATH_REPORT` env var.
///
/// # Examples
///
/// Basic usage with default settings (P95 percentile, table format):
///
/// ```rust,no_run
/// #[hotpath::main]
/// fn main() {
///     // Your code here
/// }
/// ```
///
/// Custom percentiles:
///
/// ```rust,no_run
/// #[tokio::main]
/// #[hotpath::main(percentiles = [50, 90, 95, 99])]
/// async fn main() {
///     // Your code here
/// }
/// ```
///
/// JSON output to file:
///
/// ```rust,no_run
/// #[hotpath::main(format = "json-pretty", output_path = "report.json")]
/// fn main() {
///     // Your code here
/// }
/// ```
///
/// Select report sections:
///
/// ```rust,no_run
/// #[hotpath::main(report = "functions-timing,channels")]
/// fn main() {
///     // Your code here
/// }
/// ```
///
/// # Usage with Tokio
///
/// When using with tokio, place `#[tokio::main]` before `#[hotpath::main]`:
///
/// ```rust,no_run
/// #[tokio::main]
/// #[hotpath::main]
/// async fn main() {
///     // Your code here
/// }
/// ```
///
/// # Limitations
///
/// Only one hotpath guard can be active at a time. Creating a second guard (either via this
/// macro or via [`HotpathGuardBuilder`](../hotpath/struct.HotpathGuardBuilder.html)) will cause a panic.
///
/// # See Also
///
/// * [`measure`](macro@measure) - Attribute macro for instrumenting functions
/// * [`measure_block!`](../hotpath/macro.measure_block.html) - Macro for measuring code blocks
/// * [`HotpathGuardBuilder`](../hotpath/struct.HotpathGuardBuilder.html) - Programmatic alternative to this macro
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(feature = "hotpath")]
    {
        lib_on::main_impl(attr, item)
    }
    #[cfg(not(feature = "hotpath"))]
    {
        lib_off::main_impl(attr, item)
    }
}

/// Instruments a function to measure execution time or memory allocations.
///
/// Automatically detects sync vs async and inserts the appropriate measurement guard.
/// Compiles to zero overhead when the `hotpath` feature is disabled.
///
/// # Measurements
///
/// * **Time profiling** (default) — execution duration via high-precision timers
/// * **Allocation profiling** (`hotpath-alloc` feature) — bytes allocated and allocation count
///
/// # Parameters
///
/// * `log` - If `true`, logs the return value on each call (requires `Debug` on return type)
/// * `future` - If `true`, also tracks the future lifecycle (poll count, state transitions, cancellation). Only valid on async functions.
///
/// # Examples
///
/// ```rust,no_run
/// #[hotpath::measure]
/// fn process(data: &[u8]) -> usize {
///     data.len()
/// }
///
/// #[hotpath::measure(log = true)]
/// fn compute() -> i32 {
///     42
/// }
///
/// #[hotpath::measure(future = true)]
/// async fn fetch_data() -> Vec<u8> {
///     vec![1, 2, 3]
/// }
/// ```
///
/// # Async Allocation Limitation
///
/// Allocation profiling requires `current_thread` tokio runtime because thread-local
/// tracking cannot follow tasks across threads. Time profiling works with any runtime.
///
/// # See Also
///
/// * [`main`](macro@main) - Initializes the profiling system
/// * [`measure_all`](macro@measure_all) - Bulk instrumentation for modules and impl blocks
/// * [`measure_block!`](../hotpath/macro.measure_block.html) - Instruments code blocks
#[proc_macro_attribute]
pub fn measure(attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(feature = "hotpath")]
    {
        lib_on::measure_impl(attr, item)
    }
    #[cfg(not(feature = "hotpath"))]
    {
        lib_off::measure_impl(attr, item)
    }
}

/// Instruments an async function to track its lifecycle as a Future.
///
/// Wraps the function body with the `future!` macro to track poll counts,
/// state transitions (pending/ready/cancelled), and optionally the output value.
/// Can only be applied to `async fn`.
///
/// # Parameters
///
/// * `log` - If `true`, logs the output value on completion (requires `Debug` on return type)
///
/// # Examples
///
/// ```rust,no_run
/// #[hotpath::future_fn]
/// async fn fetch_data() -> Vec<u8> {
///     vec![1, 2, 3]
/// }
///
/// #[hotpath::future_fn(log = true)]
/// async fn compute() -> i32 {
///     42
/// }
/// ```
///
/// # See Also
///
/// * [`measure`](macro@measure) - Instruments execution time / allocations
/// * [`future!`](../hotpath/macro.future.html) - Declarative macro for wrapping future expressions
#[proc_macro_attribute]
pub fn future_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(feature = "hotpath")]
    {
        lib_on::future_fn_impl(attr, item)
    }
    #[cfg(not(feature = "hotpath"))]
    {
        lib_off::future_fn_impl(attr, item)
    }
}

/// Marks a function to be excluded from profiling when used with [`measure_all`](macro@measure_all).
///
/// # Usage
///
/// ```rust,no_run
/// #[hotpath::measure_all]
/// impl MyStruct {
///     fn important_method(&self) {
///         // This will be measured
///     }
///
///     #[hotpath::skip]
///     fn not_so_important_method(&self) -> usize {
///         // This will NOT be measured
///         self.value
///     }
/// }
/// ```
///
/// # See Also
///
/// * [`measure_all`](macro@measure_all) - Bulk instrumentation macro
/// * [`measure`](macro@measure) - Individual function instrumentation
#[proc_macro_attribute]
pub fn skip(attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(feature = "hotpath")]
    {
        lib_on::skip_impl(attr, item)
    }
    #[cfg(not(feature = "hotpath"))]
    {
        lib_off::skip_impl(attr, item)
    }
}

/// Instruments all functions in a module or impl block with the `measure` profiling macro.
///
/// This attribute macro applies the [`measure`](macro@measure) macro to every function
/// in the annotated module or impl block, providing bulk instrumentation without needing
/// to annotate each function individually.
///
/// # Usage
///
/// On modules:
///
/// ```rust,no_run
/// #[hotpath::measure_all]
/// mod my_module {
///     fn function_one() {
///         // This will be automatically measured
///     }
///
///     fn function_two() {
///         // This will also be automatically measured
///     }
/// }
/// ```
///
/// On impl blocks:
///
/// ```rust,no_run
/// struct MyStruct;
///
/// #[hotpath::measure_all]
/// impl MyStruct {
///     fn method_one(&self) {
///         // This will be automatically measured
///     }
///
///     fn method_two(&self) {
///         // This will also be automatically measured
///     }
/// }
/// ```
///
/// # See Also
///
/// * [`measure`](macro@measure) - Attribute macro for instrumenting individual functions
/// * [`main`](macro@main) - Attribute macro that initializes profiling
/// * [`skip`](macro@skip) - Marker to exclude specific functions from measurement
#[proc_macro_attribute]
pub fn measure_all(attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(feature = "hotpath")]
    {
        lib_on::measure_all_impl(attr, item)
    }
    #[cfg(not(feature = "hotpath"))]
    {
        lib_off::measure_all_impl(attr, item)
    }
}
