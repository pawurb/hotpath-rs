use proc_macro::TokenStream;
use quote::quote;
use std::sync::LazyLock;
use syn::parse::Parser;
use syn::{parse_macro_input, ImplItem, Item, ItemFn, Lit, LitInt, LitStr, Path};

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

static KEEP_INLINE: LazyLock<bool> = LazyLock::new(|| env_flag("HOTPATH_KEEP_INLINE"));

#[derive(Clone, Copy)]
pub(crate) enum Format {
    Table,
    Json,
    JsonPretty,
    None,
}

impl Format {
    pub(crate) fn to_tokens(self) -> proc_macro2::TokenStream {
        match self {
            Format::Table => quote!(hotpath::Format::Table),
            Format::Json => quote!(hotpath::Format::Json),
            Format::JsonPretty => quote!(hotpath::Format::JsonPretty),
            Format::None => quote!(hotpath::Format::None),
        }
    }
}

/// Initializes the hotpath profiling system and generates a performance report on program exit.
///
/// This attribute macro should be applied to your program's main (or other entry point) function to enable profiling.
/// It creates a guard that initializes the background measurement processing thread and
/// automatically displays a performance summary when the program exits.
/// Additionally it creates a measurement guard that will be used to measure the wrapper function itself.
///
/// # Parameters
///
/// * `percentiles` - Array of percentile values (0.0-100.0) to display in the report, e.g. `[50, 95, 99.9]`. Default: `[95]`
/// * `format` - Output format as a string: `"table"` (default), `"json"`, `"json-pretty"`, or `"none"`
/// * `limit` - Global maximum number of items shown in each report section (functions, channels, streams, futures, threads, rw_locks). `0` = unlimited.
/// * `functions_limit` - Maximum number of functions shown in the report. Overrides `limit` for functions.
/// * `channels_limit` - Maximum number of channels shown in the report. Overrides `limit` for channels.
/// * `streams_limit` - Maximum number of streams shown in the report. Overrides `limit` for streams.
/// * `futures_limit` - Maximum number of futures shown in the report. Overrides `limit` for futures.
/// * `threads_limit` - Maximum number of threads shown in the report. Overrides `limit` for threads.
/// * `output_path` - File path for the report. Defaults to stdout. Overridden by `HOTPATH_OUTPUT_PATH` env var.
/// * `report` - Comma-separated sections to include. Overridden by `HOTPATH_REPORT` env var.
/// * `allocator` - Optional allocator type path used when `hotpath-alloc` is enabled.
///   Defaults to `std::alloc::System`.
///
/// Environment variable precedence for report output:
/// `HOTPATH_LIMIT`, `HOTPATH_FUNCTIONS_LIMIT`, `HOTPATH_CHANNELS_LIMIT`,
/// `HOTPATH_STREAMS_LIMIT`, `HOTPATH_FUTURES_LIMIT`, and `HOTPATH_THREADS_LIMIT`
/// override the matching macro arguments. Per-resource env vars override `HOTPATH_LIMIT`.
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
/// #[hotpath::main(percentiles = [50, 90, 95, 99.9])]
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
/// #[hotpath::main(percentiles = [50, 99.9], format = "json")]
/// fn main() {
///     // Your code here
/// }
/// ```
///
/// Global limit (applies to all report sections):
///
/// ```rust,no_run
/// #[hotpath::main(limit = 20)]
/// fn main() {
///     // Your code here
/// }
/// ```
///
/// Per-resource limits override the global limit:
///
/// ```rust,no_run
/// #[hotpath::main(limit = 10, functions_limit = 20, channels_limit = 5)]
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
/// * [`HotpathGuardBuilder`](../hotpath/struct.HotpathGuardBuilder.html) - Manual control over profiling lifecycle
pub fn main_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;

    // Defaults
    let mut percentiles: Vec<f64> = vec![95.0];
    let mut format = Format::Table;
    let mut global_limit: Option<usize> = None;
    let mut functions_limit: Option<usize> = None;
    let mut channels_limit: Option<usize> = None;
    let mut streams_limit: Option<usize> = None;
    let mut futures_limit: Option<usize> = None;
    let mut threads_limit: Option<usize> = None;
    let mut rw_locks_limit: Option<usize> = None;
    let mut mutexes_limit: Option<usize> = None;
    let mut output_path: Option<String> = None;
    let mut report_sections: Option<String> = None;
    let mut allocator: Option<Path> = None;

    // Parse named args like: percentiles=[..], format="..", report=".."
    if !attr.is_empty() {
        let parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("percentiles") {
                meta.input.parse::<syn::Token![=]>()?;
                let content;
                syn::bracketed!(content in meta.input);
                let mut vals = Vec::new();
                while !content.is_empty() {
                    let lit: Lit = content.parse()?;
                    let v: f64 = match &lit {
                        Lit::Int(li) => li.base10_parse::<u64>().map(|i| i as f64)?,
                        Lit::Float(lf) => lf.base10_parse()?,
                        _ => return Err(meta.error("Expected a number for percentile")),
                    };
                    if !(0.0..=100.0).contains(&v) {
                        return Err(
                            meta.error(format!("Invalid percentile {} (must be 0..=100)", v))
                        );
                    }
                    vals.push(v);
                    if !content.is_empty() {
                        content.parse::<syn::Token![,]>()?;
                    }
                }
                if vals.is_empty() {
                    return Err(meta.error("At least one percentile must be specified"));
                }
                percentiles = vals;
                return Ok(());
            }

            if meta.path.is_ident("format") {
                meta.input.parse::<syn::Token![=]>()?;
                let lit: LitStr = meta.input.parse()?;
                format =
                    match lit.value().as_str() {
                        "table" => Format::Table,
                        "json" => Format::Json,
                        "json-pretty" => Format::JsonPretty,
                        "none" => Format::None,
                        other => return Err(meta.error(format!(
                            "Unknown format {:?}. Expected one of: \"table\", \"json\", \"json-pretty\", \"none\"",
                            other
                        ))),
                    };
                return Ok(());
            }

            if meta.path.is_ident("limit") {
                meta.input.parse::<syn::Token![=]>()?;
                let li: LitInt = meta.input.parse()?;
                global_limit = Some(li.base10_parse()?);
                return Ok(());
            }

            if meta.path.is_ident("functions_limit") {
                meta.input.parse::<syn::Token![=]>()?;
                let li: LitInt = meta.input.parse()?;
                functions_limit = Some(li.base10_parse()?);
                return Ok(());
            }

            if meta.path.is_ident("channels_limit") {
                meta.input.parse::<syn::Token![=]>()?;
                let li: LitInt = meta.input.parse()?;
                channels_limit = Some(li.base10_parse()?);
                return Ok(());
            }

            if meta.path.is_ident("streams_limit") {
                meta.input.parse::<syn::Token![=]>()?;
                let li: LitInt = meta.input.parse()?;
                streams_limit = Some(li.base10_parse()?);
                return Ok(());
            }

            if meta.path.is_ident("futures_limit") {
                meta.input.parse::<syn::Token![=]>()?;
                let li: LitInt = meta.input.parse()?;
                futures_limit = Some(li.base10_parse()?);
                return Ok(());
            }

            if meta.path.is_ident("threads_limit") {
                meta.input.parse::<syn::Token![=]>()?;
                let li: LitInt = meta.input.parse()?;
                threads_limit = Some(li.base10_parse()?);
                return Ok(());
            }

            if meta.path.is_ident("rw_locks_limit") {
                meta.input.parse::<syn::Token![=]>()?;
                let li: LitInt = meta.input.parse()?;
                rw_locks_limit = Some(li.base10_parse()?);
                return Ok(());
            }

            if meta.path.is_ident("mutexes_limit") {
                meta.input.parse::<syn::Token![=]>()?;
                let li: LitInt = meta.input.parse()?;
                mutexes_limit = Some(li.base10_parse()?);
                return Ok(());
            }

            if meta.path.is_ident("output_path") {
                meta.input.parse::<syn::Token![=]>()?;
                let lit: LitStr = meta.input.parse()?;
                output_path = Some(lit.value());
                return Ok(());
            }

            if meta.path.is_ident("report") {
                meta.input.parse::<syn::Token![=]>()?;
                let lit: LitStr = meta.input.parse()?;
                report_sections = Some(lit.value());
                return Ok(());
            }

            if meta.path.is_ident("allocator") {
                meta.input.parse::<syn::Token![=]>()?;
                allocator = Some(meta.input.parse()?);
                return Ok(());
            }

            Err(meta.error(
                "Unknown parameter. Supported: percentiles=[..], format=\"..\", limit=N, functions_limit=N, channels_limit=N, streams_limit=N, futures_limit=N, threads_limit=N, rw_locks_limit=N, mutexes_limit=N, output_path=\"..\", report=\"..\", allocator=TypePath",
            ))
        });

        if let Err(e) = parser.parse2(proc_macro2::TokenStream::from(attr)) {
            return e.to_compile_error().into();
        }
    }

    let percentiles_array = quote! { &[#(#percentiles),*] };
    let format_token = format.to_tokens();

    let asyncness = sig.asyncness.is_some();
    let fn_name = &sig.ident;

    let output_path_call = match &output_path {
        Some(path) => quote! { .output_path(#path) },
        None => quote! {},
    };

    let sections_call = if let Some(ref report_str) = report_sections {
        let section_tokens: Vec<proc_macro2::TokenStream> = report_str
            .split(',')
            .filter_map(|s| {
                let s = s.trim();
                match s {
                    "functions-timing" => Some(quote! { hotpath::Section::FunctionsTiming }),
                    "functions-alloc" => Some(quote! { hotpath::Section::FunctionsAlloc }),
                    "channels" => Some(quote! { hotpath::Section::Channels }),
                    "streams" => Some(quote! { hotpath::Section::Streams }),
                    "futures" => Some(quote! { hotpath::Section::Futures }),
                    "threads" => Some(quote! { hotpath::Section::Threads }),
                    "rw_locks" => Some(quote! { hotpath::Section::RwLocks }),
                    "all" => None, // handled separately
                    _ => None,
                }
            })
            .collect();

        if report_str.split(',').any(|s| s.trim() == "all") {
            quote! { .sections(hotpath::Section::all()) }
        } else if !section_tokens.is_empty() {
            quote! { .sections(vec![#(#section_tokens),*]) }
        } else {
            quote! {}
        }
    } else {
        quote! {}
    };

    let caller_name_init = quote! {
        let caller_name: &'static str =
            concat!(module_path!(), "::", stringify!(#fn_name));
    };

    let global_limit_call = match global_limit {
        Some(l) => quote! { .limit(#l) },
        None => quote! {},
    };
    let functions_limit_call = match functions_limit {
        Some(l) => quote! { .functions_limit(#l) },
        None => quote! {},
    };
    let channels_limit_call = match channels_limit {
        Some(l) => quote! { .channels_limit(#l) },
        None => quote! {},
    };
    let streams_limit_call = match streams_limit {
        Some(l) => quote! { .streams_limit(#l) },
        None => quote! {},
    };
    let futures_limit_call = match futures_limit {
        Some(l) => quote! { .futures_limit(#l) },
        None => quote! {},
    };
    let threads_limit_call = match threads_limit {
        Some(l) => quote! { .threads_limit(#l) },
        None => quote! {},
    };
    let rw_locks_limit_call = match rw_locks_limit {
        Some(l) => quote! { .rw_locks_limit(#l) },
        None => quote! {},
    };
    let mutexes_limit_call = match mutexes_limit {
        Some(l) => quote! { .mutexes_limit(#l) },
        None => quote! {},
    };
    let builder_chain = quote! {
        hotpath::HotpathGuardBuilder::new(caller_name)
            .percentiles(#percentiles_array)
            #global_limit_call
            #functions_limit_call
            #channels_limit_call
            #streams_limit_call
            #futures_limit_call
            #threads_limit_call
            #rw_locks_limit_call
            #mutexes_limit_call
            .format(#format_token)
            #output_path_call
            #sections_call
    };

    let guard_init = quote! {
        let _hotpath: Option<hotpath::HotpathGuard> = {
            #caller_name_init
            let builder = #builder_chain;
            match std::env::var("HOTPATH_SHUTDOWN_MS").ok().and_then(|v| v.parse::<u64>().ok()) {
                Some(ms) => {
                    builder.build_with_shutdown(std::time::Duration::from_millis(ms));
                    None
                }
                None => Some(builder.build()),
            }
        };
    };

    let allocator_path = allocator
        .map(|path| quote! { #path })
        .unwrap_or_else(|| quote! { ::std::alloc::System });

    let allocator_item = quote! {
        #[cfg(feature = "hotpath-alloc")]
        #[global_allocator]
        static __HOTPATH_GLOBAL_ALLOCATOR: hotpath::CountingAllocator<#allocator_path> =
            hotpath::CountingAllocator::new();
    };

    let body = quote! {
        #guard_init
        #block
    };

    let wrapped_body = if asyncness {
        quote! { async { #body }.await }
    } else {
        body
    };

    let output = quote! {
        #allocator_item
        #vis #sig {
            #wrapped_body
        }
    };

    output.into()
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
/// When the `hotpath` feature is disabled, this macro compiles to zero overhead (no instrumentation).
///
/// # Parameters
///
/// * `log` - If `true`, logs the result value when the function returns (requires `Debug` on return type)
/// * `future` - If `true`, also tracks async future lifecycle. Only valid on async functions.
/// * `label` - String literal that replaces the full reported identifier
///   (instead of `module_path::<fn_name>`).
/// * `impl_type` - String literal naming the enclosing impl type (e.g. `"Worker"`).
///   Inserts the type segment so the registered name is `module_path::<Type>::<fn_name>`.
///   Use this for bare `#[hotpath::measure]` on a method inside an `impl` block when the
///   impl is not covered by [`measure_all`](macro@measure_all). Required for correct CPU
///   sampling attribution under `hotpath-cpu`.
///
///   Trait impl methods (`impl Trait for Type`) still won't attribute correctly under
///   `hotpath-cpu` even with `impl_type` — their demangled symbols use the
///   `<Type as Trait>::method` form.
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
/// With a custom label:
///
/// ```rust,no_run
/// #[hotpath::measure(label = "db_query")]
/// fn fetch_user(id: u64) { /* ... */ }
/// ```
///
/// On a method inside an inherent impl (when not using `measure_all`):
///
/// ```rust,no_run
/// struct Worker;
/// impl Worker {
///     #[hotpath::measure(impl_type = "Worker")]
///     fn run(&self) { /* ... */ }
/// }
/// ```
///
/// # See Also
///
/// * [`main`](macro@main) - Attribute macro that initializes profiling
/// * [`measure_all`](macro@measure_all) - Bulk instrumentation; auto-injects the impl type
/// * [`measure_block!`](../hotpath/macro.measure_block.html) - Macro for measuring code blocks
pub fn measure_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let touch_inline = cfg!(feature = "hotpath-cpu") && !*KEEP_INLINE;
    let attrs: Vec<&syn::Attribute> = input
        .attrs
        .iter()
        .filter(|a| !(touch_inline && a.path().is_ident("inline")))
        .collect();
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;

    let inline_attr = if touch_inline {
        quote! { #[inline(never)] }
    } else {
        quote! {}
    };

    let fn_ident = &sig.ident;
    let is_async_fn = sig.asyncness.is_some();

    let mut enable_result_logging = false;
    let mut enable_future_tracking = false;
    let mut label: Option<syn::LitStr> = None;
    let mut impl_type: Option<syn::LitStr> = None;

    if !attr.is_empty() {
        let parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("log") {
                meta.input.parse::<syn::Token![=]>()?;
                let lit: syn::LitBool = meta.input.parse()?;
                enable_result_logging = lit.value();
                return Ok(());
            }

            if meta.path.is_ident("future") {
                meta.input.parse::<syn::Token![=]>()?;
                let lit: syn::LitBool = meta.input.parse()?;
                enable_future_tracking = lit.value();
                return Ok(());
            }

            if meta.path.is_ident("label") {
                meta.input.parse::<syn::Token![=]>()?;
                let lit: syn::LitStr = meta.input.parse()?;
                if lit.value().is_empty() {
                    return Err(meta.error("label must be a non-empty string literal"));
                }
                label = Some(lit);
                return Ok(());
            }

            if meta.path.is_ident("impl_type") {
                meta.input.parse::<syn::Token![=]>()?;
                let lit: syn::LitStr = meta.input.parse()?;
                if lit.value().is_empty() {
                    return Err(meta.error("impl_type must be a non-empty string literal"));
                }
                impl_type = Some(lit);
                return Ok(());
            }

            Err(meta.error(
                "Unknown parameter. Supported: log = true, future = true, label = \"name\", impl_type = \"Type\"",
            ))
        });

        if let Err(e) = parser.parse2(proc_macro2::TokenStream::from(attr)) {
            return e.to_compile_error().into();
        }
    }

    if enable_future_tracking && !is_async_fn {
        return syn::Error::new_spanned(
            sig.fn_token,
            "future = true can only be used on async functions",
        )
        .to_compile_error()
        .into();
    }

    let measurement_loc = match (&label, &impl_type) {
        (Some(lit), _) => quote! { #lit },
        (None, Some(ty)) => {
            quote! { concat!(module_path!(), "::", #ty, "::", stringify!(#fn_ident)) }
        }
        (None, None) => quote! { concat!(module_path!(), "::", stringify!(#fn_ident)) },
    };

    let cpu_alias_register = if label.is_some() {
        let symbol_loc = match &impl_type {
            Some(ty) => {
                quote! { concat!(module_path!(), "::", #ty, "::", stringify!(#fn_ident)) }
            }
            None => quote! { concat!(module_path!(), "::", stringify!(#fn_ident)) },
        };
        quote! {
            {
                static __HOTPATH_CPU_ALIAS_ONCE: ::std::sync::Once = ::std::sync::Once::new();
                __HOTPATH_CPU_ALIAS_ONCE.call_once(|| {
                    hotpath::functions::register_cpu_label_alias(#measurement_loc, #symbol_loc);
                });
            }
        }
    } else {
        quote! {}
    };

    let wrapped_body = if !is_async_fn {
        if enable_result_logging {
            quote! {
                hotpath::functions::measure_sync_log(#measurement_loc, || #block)
            }
        } else {
            quote! {
                let _guard = hotpath::functions::build_measurement_guard_sync(#measurement_loc, false);
                #block
            }
        }
    } else if enable_future_tracking {
        if enable_result_logging {
            quote! {
                hotpath::functions::measure_async_future_log(#measurement_loc, async #block).await
            }
        } else {
            quote! {
                hotpath::functions::measure_async_future(#measurement_loc, async #block).await
            }
        }
    } else if enable_result_logging {
        quote! {
            hotpath::functions::measure_async_log(#measurement_loc, async #block).await
        }
    } else {
        quote! {
            hotpath::functions::measure_async(#measurement_loc, async #block).await
        }
    };

    let output = quote! {
        #inline_attr
        #(#attrs)*
        #vis #sig {
            #cpu_alias_register
            #wrapped_body
        }
    };

    output.into()
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
pub fn future_fn_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let touch_inline = cfg!(feature = "hotpath-cpu") && !*KEEP_INLINE;
    let attrs: Vec<&syn::Attribute> = input
        .attrs
        .iter()
        .filter(|a| !(touch_inline && a.path().is_ident("inline")))
        .collect();
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;

    let inline_attr = if touch_inline {
        quote! { #[inline(never)] }
    } else {
        quote! {}
    };

    // Ensure the function is async
    if sig.asyncness.is_none() {
        return syn::Error::new_spanned(
            sig.fn_token,
            "The #[future_fn] attribute can only be applied to async functions",
        )
        .to_compile_error()
        .into();
    }

    // Parse optional `log = true` attribute
    let mut log_result = false;

    if !attr.is_empty() {
        let parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("log") {
                meta.input.parse::<syn::Token![=]>()?;
                let lit: syn::LitBool = meta.input.parse()?;
                log_result = lit.value();
                return Ok(());
            }

            Err(meta.error("Unknown parameter. Supported: log = true"))
        });

        if let Err(e) = parser.parse2(proc_macro2::TokenStream::from(attr)) {
            return e.to_compile_error().into();
        }
    }

    let fn_name = &sig.ident;

    // Generate the wrapped body using the future! macro pattern
    let wrapped_body = if log_result {
        quote! {
            {
                const FUTURE_LOC: &'static str = concat!(module_path!(), "::", stringify!(#fn_name));
                hotpath::futures::init_futures_state();
                hotpath::InstrumentFutureLog::instrument_future_log(
                    async #block,
                    FUTURE_LOC,
                    None
                ).await
            }
        }
    } else {
        quote! {
            {
                const FUTURE_LOC: &'static str = concat!(module_path!(), "::", stringify!(#fn_name));
                hotpath::futures::init_futures_state();
                hotpath::InstrumentFuture::instrument_future(
                    async #block,
                    FUTURE_LOC,
                    None
                ).await
            }
        }
    };

    let output = quote! {
        #inline_attr
        #(#attrs)*
        #vis #sig {
            #wrapped_body
        }
    };

    output.into()
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
pub fn skip_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
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
/// On inherent impl blocks the type segment is auto-injected, so methods are registered
/// as `module_path::<Type>::<method>` — matching the demangled symbol used by
/// `hotpath-cpu` sampling attribution.
///
/// Trait impls (`impl Trait for Type`) are also instrumented but their demangled symbols
/// use the `<Type as Trait>::method` form, so CPU sampling attribution will not match
/// for trait methods.
///
/// # See Also
///
/// * [`measure`](macro@measure) - Attribute macro for instrumenting individual functions
/// * [`main`](macro@main) - Attribute macro that initializes profiling
/// * [`skip`](macro@skip) - Marker to exclude specific functions from measurement
pub fn measure_all_impl(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let parsed_item = parse_macro_input!(item as Item);

    match parsed_item {
        Item::Mod(mut module) => {
            if let Some((_brace, items)) = &mut module.content {
                for it in items.iter_mut() {
                    if let Item::Fn(func) = it {
                        if !has_hotpath_skip_or_measure(&func.attrs) {
                            let func_tokens = TokenStream::from(quote!(#func));
                            let transformed = measure_impl(TokenStream::new(), func_tokens);
                            *func = syn::parse_macro_input!(transformed as ItemFn);
                        }
                    }
                }
            }
            TokenStream::from(quote!(#module))
        }
        Item::Impl(mut impl_block) => {
            let type_name = extract_self_type_name(&impl_block.self_ty);
            for item in impl_block.items.iter_mut() {
                if let ImplItem::Fn(method) = item {
                    if !has_hotpath_skip_or_measure(&method.attrs) {
                        let func_tokens = TokenStream::from(quote!(#method));
                        let attr_ts = match &type_name {
                            Some(name) => TokenStream::from(quote!(impl_type = #name)),
                            None => TokenStream::new(),
                        };
                        let transformed = measure_impl(attr_ts, func_tokens);
                        *method = syn::parse_macro_input!(transformed as syn::ImplItemFn);
                    }
                }
            }
            TokenStream::from(quote!(#impl_block))
        }
        other => syn::Error::new_spanned(
            other,
            "measure_all can only be applied to modules or impl blocks",
        )
        .to_compile_error()
        .into(),
    }
}

fn extract_self_type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

fn has_hotpath_skip_or_measure(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();

        // Check for #[hotpath::skip]
        if path.segments.len() == 2
            && path.segments[0].ident == "hotpath"
            && path.segments[1].ident == "skip"
        {
            return true;
        }

        // Check for #[hotpath::measure]
        if path.segments.len() == 2
            && path.segments[0].ident == "hotpath"
            && path.segments[1].ident == "measure"
        {
            return true;
        }

        // Check for #[cfg_attr(..., hotpath::skip)] or #[cfg_attr(..., hotpath::measure)]
        if path.is_ident("cfg_attr") {
            let attr_str = quote!(#attr).to_string();
            if attr_str.contains("hotpath")
                && (attr_str.contains("skip") || attr_str.contains("measure"))
            {
                return true;
            }
        }

        false
    })
}
