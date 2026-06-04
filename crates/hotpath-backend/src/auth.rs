use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::{IntoResponse, Redirect},
    routing::get,
};
use cookie::{Cookie, SameSite};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl, basic::BasicClient, reqwest::async_http_client,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct GitHubUser {
    pub(crate) login: String,
    #[serde(default)]
    pub(crate) email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

#[derive(Clone)]
pub(crate) struct AuthState {
    github_client: BasicClient,
}

fn host() -> String {
    std::env::var("HOST").unwrap_or_else(|_| "https://hotpath.rs".to_string())
}

impl AuthState {
    pub(crate) fn new() -> Self {
        let github_client_id = std::env::var("GITHUB_CLIENT_ID")
            .expect("GITHUB_CLIENT_ID environment variable not set");
        let github_client_secret = std::env::var("GITHUB_CLIENT_SECRET")
            .expect("GITHUB_CLIENT_SECRET environment variable not set");

        let redirect_url = format!("{}/auth/github/callback", host());

        let github_client = BasicClient::new(
            ClientId::new(github_client_id),
            Some(ClientSecret::new(github_client_secret)),
            AuthUrl::new("https://github.com/login/oauth/authorize".to_string())
                .expect("invalid GitHub auth URL"),
            Some(
                TokenUrl::new("https://github.com/login/oauth/access_token".to_string())
                    .expect("invalid GitHub token URL"),
            ),
        )
        .set_redirect_uri(RedirectUrl::new(redirect_url).expect("invalid redirect URL"));

        Self { github_client }
    }
}

pub(crate) fn auth_routes() -> Router<AuthState> {
    Router::new()
        .route("/auth/github/login", get(login))
        .route("/auth/github/callback", get(callback))
}

#[derive(Deserialize)]
pub(crate) struct CallbackQuery {
    code: String,
    state: String,
}

async fn login(State(auth_state): State<AuthState>) -> impl IntoResponse {
    let (auth_url, csrf_token) = auth_state
        .github_client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("user:email".to_string()))
        .url();

    let csrf_cookie = Cookie::build("csrf_token", csrf_token.secret().clone())
        .path("/")
        .same_site(SameSite::Lax)
        .http_only(true)
        .finish();

    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, csrf_cookie.to_string().parse().unwrap());

    (headers, Redirect::to(auth_url.as_ref())).into_response()
}

async fn callback(
    State(auth_state): State<AuthState>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let cookies = headers
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    let mut csrf_token: Option<&str> = None;
    for cookie_str in cookies.split(';') {
        let cookie_str = cookie_str.trim();
        if cookie_str.starts_with("csrf_token=") {
            csrf_token = cookie_str.strip_prefix("csrf_token=");
            break;
        }
    }

    if csrf_token != Some(&query.state) {
        tracing::error!("CSRF token mismatch");
        return StatusCode::BAD_REQUEST.into_response();
    }

    let token_result = auth_state
        .github_client
        .exchange_code(AuthorizationCode::new(query.code))
        .request_async(async_http_client)
        .await;

    let token = match token_result {
        Ok(token) => token,
        Err(e) => {
            tracing::error!("Failed to exchange code for token: {:?}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let access_token = token.access_token().secret();

    let mut github_user = match fetch_github_user(access_token).await {
        Ok(user) => user,
        Err(e) => {
            tracing::error!("Failed to fetch GitHub user: {:?}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if github_user.email.is_none() {
        match fetch_primary_email(access_token).await {
            Ok(email) => github_user.email = email,
            Err(e) => {
                tracing::error!("Failed to fetch GitHub emails: {:?}", e);
            }
        }
    }

    if let Err(e) = send_slack_notification(&github_user).await {
        tracing::error!(
            "Failed to send Slack notification for waitlist signup {}: {:?}",
            github_user.login,
            e
        );
    }

    let clear_csrf = Cookie::build("csrf_token", "")
        .path("/")
        .max_age(cookie::time::Duration::seconds(0))
        .finish();

    let mut response_headers = HeaderMap::new();
    response_headers.append(SET_COOKIE, clear_csrf.to_string().parse().unwrap());

    (response_headers, Redirect::to("/")).into_response()
}

async fn fetch_github_user(access_token: &str) -> Result<GitHubUser, eyre::Error> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.github.com/user")
        .bearer_auth(access_token)
        .header("User-Agent", "hotpath-rs/backend")
        .send()
        .await?;

    let text = response.text().await?;
    serde_json::from_str(&text).map_err(|e| eyre::eyre!("parse user failed: {e}; raw: {text}"))
}

async fn fetch_primary_email(access_token: &str) -> Result<Option<String>, eyre::Error> {
    let client = reqwest::Client::new();
    let emails: Vec<GitHubEmail> = client
        .get("https://api.github.com/user/emails")
        .bearer_auth(access_token)
        .header("User-Agent", "hotpath-rs/backend")
        .send()
        .await?
        .json()
        .await?;

    Ok(emails
        .into_iter()
        .find(|e| e.primary && e.verified)
        .map(|e| e.email))
}

async fn send_slack_notification(user: &GitHubUser) -> Result<(), eyre::Error> {
    let webhook = std::env::var("SLACK_WEBHOOK")?;

    let text = format!(
        "🔥🦀🔥 New hotpath-rs waitlist signup\n\
         • login: {}\n\
         • email: {}",
        user.login,
        user.email.as_deref().unwrap_or("-"),
    );

    let client = reqwest::Client::new();
    let response = client
        .post(&webhook)
        .json(&serde_json::json!({ "channel": "#hotpath-waitlist", "text": text }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(eyre::eyre!("Slack webhook returned {status}: {body}"));
    }

    Ok(())
}
