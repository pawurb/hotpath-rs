use axum::{
    body::Body,
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use regex::Regex;
use std::time::Instant;
use tracing::Instrument;
use tracing::info_span;
use uuid::Uuid;

struct Faq {
    question: &'static str,
    answer: &'static str,
}

struct SeoConfig {
    path: &'static str,
    title: &'static str,
    description: &'static str,
    breadcrumb_label: &'static str,
    faqs: &'static [Faq],
}

const SEO_MAPPINGS: &[SeoConfig] = &[
    SeoConfig {
        path: "/",
        title: "hotpath-rs | Rust Performance & Memory Profiler (Async Runtime)",
        description: "Lightweight Rust profiler for performance metrics, memory allocations, channels and runtime monitoring. Profile functions, channels, futures, streams and threads with low overhead.",
        breadcrumb_label: "Home",
        faqs: &[
            Faq {
                question: "What is hotpath-rs?",
                answer: "hotpath-rs is a Rust performance profiler that instruments functions, channels, futures, and streams. It measures execution time, memory allocations, and async data flow to help find runtime bottlenecks. It is used by open-source projects including Apache OpenDAL and Apache HoraeDB.",
            },
            Faq {
                question: "Does hotpath-rs have runtime overhead when disabled?",
                answer: "No. hotpath-rs is fully gated by a Cargo feature flag. When the feature is not enabled, all macros compile to no-ops with zero compile-time and runtime overhead. All library dependencies are optional and not compiled unless profiling is enabled.",
            },
            Faq {
                question: "What metrics does hotpath-rs track?",
                answer: "hotpath-rs tracks function execution time (avg, total, p95, p99), memory allocations (bytes and count per function), channel throughput (send/receive counts, queue sizes), stream items yielded, future poll counts and lifecycle, thread CPU usage, and Tokio runtime worker stats.",
            },
        ],
    },
    SeoConfig {
        path: "/sampling_comparison",
        title: "Rust Performance Profiling: Instrumentation vs Sampling Profilers | hotpath-rs",
        description: "Compare instrumentation and sampling approaches to Rust performance profiling. See how hotpath-rs differs from perf, flamegraph, and samply for CPU-bound, blocking I/O, and async workloads.",
        breadcrumb_label: "Sampling Comparison",
        faqs: &[
            Faq {
                question: "What is the difference between instrumentation and sampling profilers in Rust?",
                answer: "Sampling profilers like perf, flamegraph, and samply periodically interrupt the program to record CPU activity. Instrumentation profilers like hotpath-rs measure the exact wall-clock execution time of annotated functions, including time spent waiting on async I/O. For CPU-bound code both approaches produce similar results, but for async and I/O-heavy workloads the numbers can differ significantly.",
            },
            Faq {
                question: "When should I use hotpath-rs instead of perf or flamegraph?",
                answer: "Use hotpath-rs when profiling async or I/O-heavy Rust applications where wall-clock latency matters more than CPU time. Sampling profilers like perf and flamegraph are better for optimizing CPU-bound hot paths. In practice, the two approaches complement each other: sampling profilers for CPU optimization, hotpath-rs for end-to-end latency analysis.",
            },
            Faq {
                question: "Why do sampling profilers show different results for async Rust code?",
                answer: "Sampling profilers only capture CPU activity, so they miss time spent awaiting I/O or async operations where no CPU work occurs. hotpath-rs measures logical execution time including wait periods. For example, in async file I/O benchmarks, hotpath-rs measured create_file at over 400% of total time (due to concurrent awaits) while samply showed only 45% for the same function.",
            },
        ],
    },
    SeoConfig {
        path: "/profiling_modes",
        title: "Rust Profiling Modes: Static Reports & Live Monitoring Dashboard | hotpath-rs",
        description: "Two ways to profile Rust performance: static reports for one-off analysis and a live monitoring dashboard for real-time runtime metrics. Track timing, memory, and data flow with hotpath-rs.",
        breadcrumb_label: "Profiling Modes",
        faqs: &[],
    },
    SeoConfig {
        path: "/functions",
        title: "Rust Function Performance Profiler: Timing & Memory Metrics | hotpath-rs",
        description: "Profile Rust function performance with precise timing metrics, memory allocation tracking, and percentile statistics. Measure sync and async functions to find and optimize runtime bottlenecks.",
        breadcrumb_label: "Functions",
        faqs: &[
            Faq {
                question: "How do I profile a Rust function with hotpath-rs?",
                answer: "Add #[hotpath::measure] to the function you want to profile and #[hotpath::main] to your main function. Then run your program with cargo run --features=hotpath. The profiler will output a table with call count, average time, percentiles (p95, p99), total time, and percentage of total runtime for each measured function.",
            },
            Faq {
                question: "Can hotpath-rs profile async Rust functions?",
                answer: "Yes. The #[hotpath::measure] macro works on both sync and async functions, measuring wall-clock execution time including await points. For memory allocation profiling of async functions, use tokio's current_thread runtime mode because the allocation tracker relies on thread-local storage.",
            },
            Faq {
                question: "How does hotpath-rs track memory allocations?",
                answer: "hotpath-rs uses a custom global allocator to intercept all memory allocations and attributes them to instrumented functions. Enable it with cargo run --features='hotpath,hotpath-alloc'. It reports per-function byte counts and allocation counts. By default tracking is cumulative (includes nested calls); set HOTPATH_ALLOC_SELF=true for exclusive (direct-only) allocations.",
            },
        ],
    },
    SeoConfig {
        path: "/futures",
        title: "Async Rust Performance Profiler: Future Monitoring & Poll Metrics | hotpath-rs",
        description: "Monitor async Rust futures with poll counts, completion tracking, and performance metrics. Debug async bottlenecks and optimize future execution patterns with hotpath-rs.",
        breadcrumb_label: "Futures",
        faqs: &[],
    },
    SeoConfig {
        path: "/channels",
        title: "Rust Channels Performance Monitoring: Message Flow & Throughput Metrics | hotpath-rs",
        description: "Monitor Rust channels performance with hotpath-rs. Track tokio, crossbeam, futures, and std channel metrics including send/receive counts, queue sizes, and throughput.",
        breadcrumb_label: "Channels",
        faqs: &[],
    },
    SeoConfig {
        path: "/streams",
        title: "Rust Async Stream Profiler: Performance Monitoring & Throughput Metrics | hotpath-rs",
        description: "Profile async Rust streams with throughput metrics, item counts, and optional item logging. Monitor futures::Stream performance with the hotpath::stream! macro.",
        breadcrumb_label: "Streams",
        faqs: &[],
    },
    SeoConfig {
        path: "/threads",
        title: "Rust Thread Performance Monitoring: Per-Thread CPU & Memory Metrics | hotpath-rs",
        description: "Monitor per-thread CPU usage and memory allocation metrics in Rust applications. Track thread states, allocation counts, and system time in the hotpath-rs monitoring dashboard.",
        breadcrumb_label: "Threads",
        faqs: &[],
    },
    SeoConfig {
        path: "/tokio_runtime",
        title: "Tokio Runtime Performance Monitoring: Worker Stats, Task Metrics & Scheduling | hotpath-rs",
        description: "Monitor Tokio runtime performance with hotpath-rs. Track worker thread utilization, task scheduling, queue depths, and I/O driver metrics for real-time Rust application monitoring.",
        breadcrumb_label: "Tokio Runtime",
        faqs: &[],
    },
    SeoConfig {
        path: "/github_ci",
        title: "Rust Performance CI: Automated Benchmarking & Regression Detection | hotpath-rs",
        description: "Automate Rust performance benchmarking in GitHub Actions. Detect runtime regressions on every pull request with detailed metrics comparison using hotpath-rs CI integration.",
        breadcrumb_label: "GitHub CI",
        faqs: &[],
    },
    SeoConfig {
        path: "/configuration",
        title: "Rust Profiler Configuration: Environment Variables & Runtime Settings | hotpath-rs",
        description: "Configure hotpath-rs profiling with environment variables. Control output format, metrics server ports, MCP integration, TUI refresh intervals, function filtering, memory tracking mode, and monitoring intervals.",
        breadcrumb_label: "Configuration",
        faqs: &[],
    },
    SeoConfig {
        path: "/mcp",
        title: "AI-Powered Rust Profiling: Query Performance Metrics with LLMs via MCP | hotpath-rs",
        description: "Connect LLM agents like Claude Code to live Rust performance metrics via MCP. Query runtime profiling data, memory usage, and async operations in natural language.",
        breadcrumb_label: "MCP Integration",
        faqs: &[],
    },
];

const STATIC_EXTENSIONS: &[&str] = &[
    ".css", ".js", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".woff", ".woff2", ".ttf",
    ".eot", ".mp4", ".webm",
];

const BASE_URL: &str = "https://hotpath.rs";
const OG_IMAGE: &str = "https://hotpath.rs/images/hotpath-ferris.png";
const SOFTWARE_APP_JSON_LD: &str = r#"{"@context":"https://schema.org","@type":"SoftwareApplication","name":"hotpath-rs","applicationCategory":"DeveloperApplication","operatingSystem":"Linux, macOS, Windows","programmingLanguage":"Rust","description":"A Rust performance profiler for runtime metrics, memory allocations, and async data flow monitoring. Profile functions, channels, futures, and streams.","url":"https://hotpath.rs","downloadUrl":"https://crates.io/crates/hotpath","codeRepository":"https://github.com/pawurb/hotpath-rs","license":"https://opensource.org/licenses/MIT","author":{"@type":"Person","name":"Pawel Urbanek","url":"https://pawelurbanek.com"},"offers":{"@type":"Offer","price":"0","priceCurrency":"USD"}}"#;

pub async fn request_tracing(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();

    // Skip instrumenting static asset requests
    if STATIC_EXTENSIONS.iter().any(|ext| path.ends_with(ext)) {
        return next.run(request).await;
    }

    let uuid = Uuid::new_v4();
    let request_id = &uuid.to_string()[0..12];
    let method = request.method().clone();

    let info_span = info_span!("req", id = %request_id, method = %method, path = %path);

    async move {
        let start = Instant::now();
        let response = next.run(request).await;
        let duration = start.elapsed();

        tracing::info!(
            status = %response.status(),
            duration_ms = duration.as_millis(),
        );

        response
    }
    .instrument(info_span)
    .await
}

pub async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();
    headers.insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("X-Frame-Options", HeaderValue::from_static("SAMEORIGIN"));
    headers.insert(
        "Referrer-Policy",
        HeaderValue::from_static("no-referrer-when-downgrade"),
    );
    headers.insert(
        "Strict-Transport-Security",
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );

    response
}

pub fn init_logs() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "hotpath_backend=debug,tower_http=debug".into());

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .init();
}

pub async fn seo_titles(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();

    let seo_config = SEO_MAPPINGS.iter().find(|cfg| cfg.path == path);

    let response = next.run(request).await;

    let Some(config) = seo_config else {
        return response;
    };

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.contains("text/html") {
        return response;
    }

    let (parts, body) = response.into_parts();

    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };

    let html = String::from_utf8_lossy(&bytes);

    let title_regex = Regex::new(r"<title>[^<]*</title>").unwrap();
    let modified = title_regex.replace(&html, format!("<title>{}</title>", config.title));

    let desc_regex = Regex::new(r#"<meta name="description" content="[^"]*">"#).unwrap();
    let canonical_url = format!("{}{}", BASE_URL, path);
    let modified = desc_regex.replace(
        &modified,
        format!(
            r#"<meta name="description" content="{}">
    <link rel="canonical" href="{}">
    <meta property="og:title" content="{}">
    <meta property="og:description" content="{}">
    <meta property="og:url" content="{}">
    <meta property="og:type" content="website">
    <meta property="og:image" content="{}">
    <meta property="og:site_name" content="hotpath-rs">
    <meta property="og:locale" content="en_US">
    <meta name="twitter:card" content="summary_large_image">
    <meta name="twitter:title" content="{}">
    <meta name="twitter:description" content="{}">
    <meta name="twitter:image" content="{}">"#,
            config.description,
            canonical_url,
            config.title,
            config.description,
            canonical_url,
            OG_IMAGE,
            config.title,
            config.description,
            OG_IMAGE,
        ),
    );

    let h1_regex = Regex::new(r#"<h1 class="menu-title">([^<]*)</h1>"#).unwrap();
    let modified = h1_regex.replace(&modified, r#"<div class="menu-title">$1</div>"#);

    let mut json_ld_block = String::new();

    let breadcrumb =
        build_breadcrumb_json_ld(config.breadcrumb_label, &canonical_url, config.path == "/");
    json_ld_block.push_str(&format!(
        r#"<script type="application/ld+json">{}</script>"#,
        breadcrumb
    ));

    let entity_json_ld = if config.path == "/" {
        SOFTWARE_APP_JSON_LD.to_string()
    } else {
        build_tech_article_json_ld(config, &canonical_url)
    };
    json_ld_block.push_str(&format!(
        r#"<script type="application/ld+json">{}</script>"#,
        entity_json_ld
    ));

    if !config.faqs.is_empty() {
        let faq_json_ld = build_faq_page_json_ld(config.faqs);
        json_ld_block.push_str(&format!(
            r#"<script type="application/ld+json">{}</script>"#,
            faq_json_ld
        ));
    }

    let modified = modified.replace("</head>", &format!("{}</head>", json_ld_block));

    Response::from_parts(parts, Body::from(modified)).into_response()
}

fn build_breadcrumb_json_ld(label: &str, canonical_url: &str, is_home: bool) -> String {
    if is_home {
        return format!(
            r#"{{"@context":"https://schema.org","@type":"BreadcrumbList","itemListElement":[{{"@type":"ListItem","position":1,"name":"Home","item":"{}"}}]}}"#,
            canonical_url
        );
    }

    format!(
        r#"{{"@context":"https://schema.org","@type":"BreadcrumbList","itemListElement":[{{"@type":"ListItem","position":1,"name":"Home","item":"https://hotpath.rs/"}},{{"@type":"ListItem","position":2,"name":"{}","item":"{}"}}]}}"#,
        label, canonical_url
    )
}

fn build_faq_page_json_ld(faqs: &[Faq]) -> String {
    let entries: Vec<String> = faqs
        .iter()
        .map(|faq| {
            format!(
                r#"{{"@type":"Question","name":"{}","acceptedAnswer":{{"@type":"Answer","text":"{}"}}}}"#,
                faq.question, faq.answer
            )
        })
        .collect();

    format!(
        r#"{{"@context":"https://schema.org","@type":"FAQPage","mainEntity":[{}]}}"#,
        entries.join(",")
    )
}

fn build_tech_article_json_ld(config: &SeoConfig, canonical_url: &str) -> String {
    let headline = config.title.split(" | ").next().unwrap_or(config.title);
    format!(
        r#"{{"@context":"https://schema.org","@type":"TechArticle","headline":"{}","description":"{}","image":"{}","datePublished":"2025-01-15","dateModified":"2026-02-07","author":{{"@type":"Person","name":"Pawel Urbanek"}},"publisher":{{"@type":"Organization","name":"hotpath-rs"}},"mainEntityOfPage":"{}"}}"#,
        headline, config.description, OG_IMAGE, canonical_url
    )
}
