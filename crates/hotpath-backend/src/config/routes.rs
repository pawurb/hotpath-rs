use crate::config::middleware;
use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

struct SitemapConfig {
    page: &'static str,
    priority: &'static str,
    changefreq: &'static str,
}

const DOC_PAGES: &[SitemapConfig] = &[
    SitemapConfig {
        page: "sampling_comparison",
        priority: "0.9",
        changefreq: "monthly",
    },
    SitemapConfig {
        page: "profiling_modes",
        priority: "0.6",
        changefreq: "monthly",
    },
    SitemapConfig {
        page: "functions",
        priority: "0.8",
        changefreq: "monthly",
    },
    SitemapConfig {
        page: "futures",
        priority: "0.6",
        changefreq: "monthly",
    },
    SitemapConfig {
        page: "channels",
        priority: "0.6",
        changefreq: "monthly",
    },
    SitemapConfig {
        page: "streams",
        priority: "0.6",
        changefreq: "monthly",
    },
    SitemapConfig {
        page: "threads",
        priority: "0.6",
        changefreq: "monthly",
    },
    SitemapConfig {
        page: "tokio_runtime",
        priority: "0.6",
        changefreq: "monthly",
    },
    SitemapConfig {
        page: "mcp",
        priority: "0.6",
        changefreq: "monthly",
    },
    SitemapConfig {
        page: "github_ci",
        priority: "0.6",
        changefreq: "monthly",
    },
    SitemapConfig {
        page: "configuration",
        priority: "0.7",
        changefreq: "monthly",
    },
];

const BASE_URL: &str = "https://hotpath.rs";

#[hotpath::measure]
pub fn app() -> Router {
    use axum::middleware::from_fn;
    use tower::ServiceBuilder;

    let static_routes = Router::new()
        .nest_service("/css", ServeDir::new("html/css"))
        .nest_service("/fonts", ServeDir::new("html/fonts"))
        .nest_service("/assets", ServeDir::new("assets"))
        .route_service(
            "/favicon.ico",
            ServeFile::new("assets/favicons/favicon.ico"),
        )
        .route_service(
            "/apple-touch-icon.png",
            ServeFile::new("assets/favicons/apple-touch-icon.png"),
        )
        .layer(from_fn(set_content_type));

    let mut router = Router::new()
        .route(
            "/",
            get(|| async { serve_doc_page("introduction.html").await }),
        )
        .route("/introduction", get(|| async { Redirect::permanent("/") }))
        .route(
            "/introduction.html",
            get(|| async { Redirect::permanent("/") }),
        )
        .route("/index.html", get(|| async { Redirect::permanent("/") }));

    for entry in DOC_PAGES {
        let html_file = format!("{}.html", entry.page);
        let clean_path = format!("/{}", entry.page);
        let html_path = format!("/{}.html", entry.page);
        let redirect_target = clean_path.clone();

        router = router.route(
            &clean_path,
            get(move || async move { serve_doc_page_owned(html_file).await }),
        );
        router = router.route(
            &html_path,
            get(move || async move { Redirect::permanent(&redirect_target) }),
        );
    }

    router
        .route("/health", get(health_check))
        .route("/robots.txt", get(robots_txt))
        .route("/sitemap.xml", get(sitemap_xml))
        .merge(static_routes)
        .fallback_service(
            ServiceBuilder::new()
                .layer(from_fn(set_content_type))
                .service(ServeDir::new("html").not_found_service(ServeFile::new("html/404.html"))),
        )
        .layer(from_fn(middleware::seo_titles))
}

async fn health_check() -> impl IntoResponse {
    "OK"
}

async fn robots_txt() -> impl IntoResponse {
    let content = format!(
        "User-agent: *\nAllow: /\n\nSitemap: {}/sitemap.xml\n",
        BASE_URL
    );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        content,
    )
}

async fn sitemap_xml() -> impl IntoResponse {
    let mut urls = vec![];

    let home_lastmod = get_file_lastmod("html_src/src/introduction.md").await;
    urls.push(format_sitemap_url(
        &format!("{}/", BASE_URL),
        home_lastmod,
        "1.0",
        "weekly",
    ));

    for entry in DOC_PAGES {
        let file_path = format!("html_src/src/{}.md", entry.page);
        let lastmod = get_file_lastmod(&file_path).await;
        urls.push(format_sitemap_url(
            &format!("{}/{}", BASE_URL, entry.page),
            lastmod,
            entry.priority,
            entry.changefreq,
        ));
    }

    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
{}
</urlset>
"#,
        urls.join("\n")
    );

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        content,
    )
}

fn format_sitemap_url(
    loc: &str,
    lastmod: Option<String>,
    priority: &str,
    changefreq: &str,
) -> String {
    let lastmod_tag = lastmod
        .map(|date| format!("\n    <lastmod>{}</lastmod>", date))
        .unwrap_or_default();
    format!(
        "  <url>\n    <loc>{}</loc>{}\n    <changefreq>{}</changefreq>\n    <priority>{}</priority>\n  </url>",
        loc, lastmod_tag, changefreq, priority
    )
}

async fn get_file_lastmod(path: &str) -> Option<String> {
    let metadata = tokio::fs::metadata(path).await.ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    let datetime = time::OffsetDateTime::from_unix_timestamp(duration.as_secs() as i64).ok()?;
    Some(format!(
        "{:04}-{:02}-{:02}",
        datetime.year(),
        datetime.month() as u8,
        datetime.day()
    ))
}

#[hotpath::measure]
async fn serve_doc_page(filename: &str) -> Response<Body> {
    let path = PathBuf::from("html").join(filename);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => html_response(content, StatusCode::OK),
        Err(_) => html_response("Page not found".to_string(), StatusCode::NOT_FOUND),
    }
}

async fn serve_doc_page_owned(filename: String) -> Response<Body> {
    serve_doc_page(&filename).await
}

#[hotpath::measure]
pub fn html_response(body: String, status: StatusCode) -> Response<Body> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    (status, headers, body).into_response()
}

#[hotpath::measure]
async fn set_content_type(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;

    if response.headers().get(header::CONTENT_TYPE).is_none() {
        let content_type = match path.rsplit('.').next() {
            Some("html") => Some("text/html; charset=utf-8"),
            Some("css") => Some("text/css; charset=utf-8"),
            Some("js") => Some("application/javascript; charset=utf-8"),
            Some("json") => Some("application/json"),
            Some("png") => Some("image/png"),
            Some("jpg") | Some("jpeg") => Some("image/jpeg"),
            Some("gif") => Some("image/gif"),
            Some("webp") => Some("image/webp"),
            Some("mp4") => Some("video/mp4"),
            Some("webm") => Some("video/webm"),
            Some("svg") => Some("image/svg+xml"),
            Some("ico") => Some("image/x-icon"),
            Some("woff") => Some("font/woff"),
            Some("woff2") => Some("font/woff2"),
            Some("ttf") => Some("font/ttf"),
            Some("eot") => Some("application/vnd.ms-fontobject"),
            _ => None,
        };

        if let Some(ct) = content_type {
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static(ct));
        }
    }

    if response.headers().get(header::CACHE_CONTROL).is_none() {
        let is_static = matches!(
            path.rsplit('.').next(),
            Some(
                "css"
                    | "js"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "svg"
                    | "ico"
                    | "woff"
                    | "woff2"
                    | "ttf"
                    | "eot"
                    | "webp"
                    | "mp4"
                    | "webm"
            )
        );

        if is_static {
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
        } else {
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=3600"),
            );
        }
    }

    response
}
