use proc_macro::TokenStream;

#[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
mod lib_on;

#[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
mod lib_off;

/// Initializes the hotpath profiling system and generates a performance report on program exit.
///
/// This attribute macro should be applied to your program's main (or other entry point) function to enable profiling.
/// It creates a guard that initializes the background measurement processing thread and
/// automatically displays a performance summary when the program exits.
/// Additionally it creates a measurement guard that will be used to measure the wrapper function itself.
///
/// # Parameters
///
/// * `percentiles` - Array of percentile values (0-100) to display in the report. Default: `[95]`
/// * `format` - Output format as a string: `"table"` (default), `"json"`, or `"json-pretty"`
/// * `limit` - Maximum number of functions to display in the report (0 = show all). Default: `15`
/// * `timeout` - Optional timeout in milliseconds. If specified, the program will print the report and exit after the timeout.
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
/// JSON output format:
///
/// ```rust,no_run
/// #[hotpath::main(format = "json-pretty")]
/// fn main() {
///     // Your code here
/// }
/// ```
///
/// Combined parameters:
///
/// ```rust,no_run
/// #[hotpath::main(percentiles = [50, 99], format = "json")]
/// fn main() {
///     // Your code here
/// }
/// ```
///
/// Custom limit (show top 20 functions):
///
/// ```rust,no_run
/// #[hotpath::main(limit = 20)]
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
/// macro or via [`FunctionsGuardBuilder`](../hotpath/struct.FunctionsGuardBuilder.html)) will cause a panic.
///
/// # See Also
///
/// * [`measure`](macro@measure) - Attribute macro for instrumenting functions
/// * [`measure_block!`](../hotpath/macro.measure_block.html) - Macro for measuring code blocks
/// * [`FunctionsGuardBuilder`](../hotpath/struct.FunctionsGuardBuilder.html) - Manual control over profiling lifecycle
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
    {
        lib_on::main_impl(attr, item)
    }
    #[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
    {
        lib_off::main_impl(attr, item)
    }
}

/// Instruments a function to send performance measurements to the hotpath profiler.
///
/// This attribute macro wraps functions with profiling code that measures execution time
/// or memory allocations (depending on enabled feature flags). The measurements are sent
/// to a background processing thread for aggregation.
///
/// # Behavior
///
/// The macro automatically detects whether the function is sync or async and instruments
/// it appropriately. Measurements include:
///
/// * **Time profiling** (default): Execution duration using high-precision timers
/// * **Allocation profiling**: Memory allocations when allocation features are enabled
///   - `hotpath-alloc` - Total bytes allocated
///   - `hotpath-alloc` - Total allocation count
///
/// # Async Function Limitations
///
/// When using allocation profiling features with async functions, you must use the
/// `tokio` runtime in `current_thread` mode:
///
/// ```rust,no_run
/// #[tokio::main(flavor = "current_thread")]
/// async fn main() {
///     // Your async code here
/// }
/// ```
///
/// This limitation exists because allocation tracking uses thread-local storage. In multi-threaded
/// runtimes, async tasks can migrate between threads, making it impossible to accurately
/// attribute allocations to specific function calls. Time-based profiling works with any runtime flavor.
///
/// When the `hotpath` feature is disabled, this macro compiles to zero overhead (no instrumentation).
///
/// # Parameters
///
/// * `log` - If `true`, logs the result value when the function returns (requires `Debug` on return type)
///
/// # Examples
///
/// With result logging (requires Debug on return type):
///
/// ```rust,no_run
/// #[hotpath::measure(log = true)]
/// fn compute() -> i32 {
///     // The result value will be logged in TUI console
///     42
/// }
/// ```
///
/// # See Also
///
/// * [`main`](macro@main) - Attribute macro that initializes profiling
/// * [`measure_block!`](../hotpath/macro.measure_block.html) - Macro for measuring code blocks
#[proc_macro_attribute]
pub fn measure(attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
    {
        lib_on::measure_impl(attr, item)
    }
    #[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
    {
        lib_off::measure_impl(attr, item)
    }
}

/// Instruments an async function to track its lifecycle as a Future.
///
/// This attribute macro wraps async functions with the `future!` macro, enabling
/// tracking of poll counts, state transitions (pending/ready/cancelled), and
/// optionally logging the result value.
///
/// # Parameters
///
/// * `log` - If `true`, logs the result value when the future completes (requires `Debug` on return type)
///
/// # Examples
///
/// Basic usage (no Debug requirement on return type):
///
/// ```rust,no_run
/// #[hotpath::future_fn]
/// async fn fetch_data() -> Vec<u8> {
///     // This future's lifecycle will be tracked
///     vec![1, 2, 3]
/// }
/// ```
///
/// With result logging (requires Debug on return type):
///
/// ```rust,no_run
/// #[hotpath::future_fn(log = true)]
/// async fn compute() -> i32 {
///     // The result value will be logged in TUI console
///     42
/// }
/// ```
///
/// # See Also
///
/// * [`measure`](macro@measure) - Attribute macro for instrumenting sync/async function timing
/// * [`future!`](../hotpath/macro.future.html) - Declarative macro for instrumenting future expressions
#[proc_macro_attribute]
pub fn future_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
    {
        lib_on::future_fn_impl(attr, item)
    }
    #[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
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
    #[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
    {
        lib_on::skip_impl(attr, item)
    }
    #[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
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
    #[cfg(all(feature = "hotpath", not(feature = "hotpath-off")))]
    {
        lib_on::measure_all_impl(attr, item)
    }
    #[cfg(any(not(feature = "hotpath"), feature = "hotpath-off"))]
    {
        lib_off::measure_all_impl(attr, item)
    }
}
