use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::{parse_macro_input, ImplItem, Item, ItemFn, LitInt, LitStr};

#[derive(Clone, Copy)]
enum Format {
    Table,
    Json,
    JsonPretty,
}

impl Format {
    fn to_tokens(self) -> proc_macro2::TokenStream {
        match self {
            Format::Table => quote!(hotpath::Format::Table),
            Format::Json => quote!(hotpath::Format::Json),
            Format::JsonPretty => quote!(hotpath::Format::JsonPretty),
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
/// #[cfg_attr(feature = "hotpath", hotpath::main)]
/// fn main() {
///     // Your code here
/// }
/// ```
///
/// Custom percentiles:
///
/// ```rust,no_run
/// #[tokio::main]
/// #[cfg_attr(feature = "hotpath", hotpath::main(percentiles = [50, 90, 95, 99]))]
/// async fn main() {
///     // Your code here
/// }
/// ```
///
/// JSON output format:
///
/// ```rust,no_run
/// #[cfg_attr(feature = "hotpath", hotpath::main(format = "json-pretty"))]
/// fn main() {
///     // Your code here
/// }
/// ```
///
/// Combined parameters:
///
/// ```rust,no_run
/// #[cfg_attr(feature = "hotpath", hotpath::main(percentiles = [50, 99], format = "json"))]
/// fn main() {
///     // Your code here
/// }
/// ```
///
/// Custom limit (show top 20 functions):
///
/// ```rust,no_run
/// #[cfg_attr(feature = "hotpath", hotpath::main(limit = 20))]
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
/// #[cfg_attr(feature = "hotpath", hotpath::main)]
/// async fn main() {
///     // Your code here
/// }
/// ```
///
/// # Limitations
///
/// Only one hotpath guard can be active at a time. Creating a second guard (either via this
/// macro or via [`GuardBuilder`](../hotpath/struct.GuardBuilder.html)) will cause a panic.
///
/// # See Also
///
/// * [`measure`](macro@measure) - Attribute macro for instrumenting functions
/// * [`measure_block!`](../hotpath/macro.measure_block.html) - Macro for measuring code blocks
/// * [`GuardBuilder`](../hotpath/struct.GuardBuilder.html) - Manual control over profiling lifecycle
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;

    // Defaults
    let mut percentiles: Vec<u8> = vec![95];
    let mut format = Format::Table;
    let mut limit: usize = 15;
    let mut timeout: Option<u64> = None;

    // Parse named args like: percentiles=[..], format=".."
    if !attr.is_empty() {
        let parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("percentiles") {
                meta.input.parse::<syn::Token![=]>()?;
                let content;
                syn::bracketed!(content in meta.input);
                let mut vals = Vec::new();
                while !content.is_empty() {
                    let li: LitInt = content.parse()?;
                    let v: u8 = li.base10_parse()?;
                    if !(0..=100).contains(&v) {
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
                        other => return Err(meta.error(format!(
                            "Unknown format {:?}. Expected one of: \"table\", \"json\", \"json-pretty\"",
                            other
                        ))),
                    };
                return Ok(());
            }

            if meta.path.is_ident("limit") {
                meta.input.parse::<syn::Token![=]>()?;
                let li: LitInt = meta.input.parse()?;
                limit = li.base10_parse()?;
                return Ok(());
            }

            if meta.path.is_ident("timeout") {
                meta.input.parse::<syn::Token![=]>()?;
                let li: LitInt = meta.input.parse()?;
                timeout = Some(li.base10_parse()?);
                return Ok(());
            }

            Err(meta.error(
                "Unknown parameter. Supported: percentiles=[..], format=\"..\", limit=N, timeout=N",
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

    let base_builder = quote! {
        let caller_name: &'static str =
            concat!(module_path!(), "::", stringify!(#fn_name));

        hotpath::GuardBuilder::new(caller_name)
            .percentiles(#percentiles_array)
            .limit(#limit)
            .format(#format_token)
    };

    let guard_init = if let Some(timeout_ms) = timeout {
        quote! {
            let _hotpath = {
                #base_builder
                    .build_with_timeout(std::time::Duration::from_millis(#timeout_ms))
            };
        }
    } else {
        quote! {
            let _hotpath = {
                #base_builder.build()
            };
        }
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
/// # See Also
///
/// * [`main`](macro@main) - Attribute macro that initializes profiling
/// * [`measure_block!`](../hotpath/macro.measure_block.html) - Macro for measuring code blocks
#[proc_macro_attribute]
pub fn measure(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let attrs = &input.attrs;

    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;

    let name = sig.ident.to_string();
    let asyncness = sig.asyncness.is_some();

    let guard_init = quote! {
        let _guard = hotpath::MeasurementGuard::build(
            concat!(module_path!(), "::", #name),
            false,
            #asyncness
        );
        #block
    };

    let wrapped = if asyncness {
        quote! { async { #guard_init }.await }
    } else {
        guard_init
    };

    let output = quote! {
        #(#attrs)*
        #vis #sig {
            #wrapped
        }
    };

    output.into()
}

/// Marks a function to be excluded from profiling when used with [`measure_all`](macro@measure_all).
///
/// # Usage
///
/// ```rust,no_run
/// #[cfg_attr(feature = "hotpath", hotpath::measure_all)]
/// impl MyStruct {
///     fn important_method(&self) {
///         // This will be measured
///     }
///
///     #[cfg_attr(feature = "hotpath", hotpath::skip)]
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
pub fn skip(_attr: TokenStream, item: TokenStream) -> TokenStream {
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
/// #[cfg_attr(feature = "hotpath", hotpath::measure_all)]
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
/// #[cfg_attr(feature = "hotpath", hotpath::measure_all)]
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
pub fn measure_all(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let parsed_item = parse_macro_input!(item as Item);

    match parsed_item {
        Item::Mod(mut module) => {
            if let Some((_brace, items)) = &mut module.content {
                for it in items.iter_mut() {
                    if let Item::Fn(func) = it {
                        if !has_hotpath_skip(&func.attrs) {
                            let func_tokens = TokenStream::from(quote!(#func));
                            let transformed = measure(TokenStream::new(), func_tokens);
                            *func = syn::parse_macro_input!(transformed as ItemFn);
                        }
                    }
                }
            }
            TokenStream::from(quote!(#module))
        }
        Item::Impl(mut impl_block) => {
            for item in impl_block.items.iter_mut() {
                if let ImplItem::Fn(method) = item {
                    if !has_hotpath_skip(&method.attrs) {
                        let func_tokens = TokenStream::from(quote!(#method));
                        let transformed = measure(TokenStream::new(), func_tokens);
                        *method = syn::parse_macro_input!(transformed as syn::ImplItemFn);
                    }
                }
            }
            TokenStream::from(quote!(#impl_block))
        }
        _ => panic!("measure_all can only be applied to modules or impl blocks"),
    }
}

fn has_hotpath_skip(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        // Check for #[skip] or #[hotpath::skip]
        if attr.path().is_ident("skip")
            || (attr.path().segments.len() == 2
                && attr.path().segments[0].ident == "hotpath"
                && attr.path().segments[1].ident == "skip")
        {
            return true;
        }

        // Check for #[cfg_attr(feature = "hotpath", hotpath::skip)]
        if attr.path().is_ident("cfg_attr") {
            let attr_str = quote!(#attr).to_string();
            if attr_str.contains("hotpath") && attr_str.contains("skip") {
                return true;
            }
        }

        false
    })
}
