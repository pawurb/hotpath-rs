use eyre::Result;
use hotpath::ProfilingMode;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct GitHubUser {
    login: String,
    #[serde(rename = "type")]
    user_type: String,
}

#[derive(Debug, Deserialize)]
struct GitHubComment {
    id: u64,
    body: String,
    user: GitHubUser,
}

fn find_existing_comment(
    repo: &str,
    pr_number: &str,
    token: &str,
    profiling_mode: &ProfilingMode,
    benchmark_id: Option<&str>,
) -> Result<Option<u64>> {
    let url = format!(
        "https://api.github.com/repos/{}/issues/{}/comments",
        repo, pr_number
    );

    let response = ureq::get(&url)
        .header("Authorization", &format!("token {}", token))
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "hotpath-utils-action")
        .call();

    match response {
        Ok(mut resp) => {
            let comments: Vec<GitHubComment> = resp.body_mut().read_json()?;

            let search_marker = match benchmark_id {
                Some(id) => format!("**Benchmark ID:** {}", id),
                None => format!("**Profiling Mode:** {}", profiling_mode),
            };

            for comment in comments {
                if comment.user.user_type == "Bot"
                    && comment.user.login == "github-actions[bot]"
                    && comment.body.contains(&search_marker)
                {
                    return Ok(Some(comment.id));
                }
            }

            Ok(None)
        }
        Err(e) => {
            println!("Warning: Failed to fetch existing comments: {}", e);
            Ok(None)
        }
    }
}

fn create_comment(repo: &str, pr_number: &str, token: &str, body: &str) -> Result<()> {
    let url = format!(
        "https://api.github.com/repos/{}/issues/{}/comments",
        repo, pr_number
    );

    let comment_body = json!({
        "body": body,
    });

    let response = ureq::post(&url)
        .header("Authorization", &format!("token {}", token))
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "hotpath-utils-action")
        .send_json(&comment_body)?;

    let status = response.status();
    if status.is_success() {
        println!("Successfully created new comment");
        Ok(())
    } else {
        let error_text = response.into_body().read_to_string()?;
        println!("Failed to create comment: {}", status);
        println!("Error details: {}", error_text);
        if status.as_u16() == 403 {
            println!("This is likely a permissions issue. Make sure the workflow has:");
            println!("permissions:");
            println!("  pull-requests: write");
            println!("  contents: read");
        }
        Err(eyre::eyre!("Failed to create comment"))
    }
}

fn update_comment(repo: &str, comment_id: u64, token: &str, body: &str) -> Result<()> {
    let url = format!(
        "https://api.github.com/repos/{}/issues/comments/{}",
        repo, comment_id
    );

    let comment_body = json!({
        "body": body,
    });

    let response = ureq::patch(&url)
        .header("Authorization", &format!("token {}", token))
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "hotpath-utils-action")
        .send_json(&comment_body)?;

    let status = response.status();
    if status.is_success() {
        println!("Successfully updated existing comment");
        Ok(())
    } else {
        let error_text = response.into_body().read_to_string()?;
        println!("Failed to update comment: {}", status);
        println!("Error details: {}", error_text);
        Err(eyre::eyre!("Failed to update comment"))
    }
}

pub fn upsert_pr_comment(
    repo: &str,
    pr_number: &str,
    token: &str,
    body: &str,
    profiling_mode: &ProfilingMode,
    benchmark_id: Option<&str>,
) -> Result<()> {
    match find_existing_comment(repo, pr_number, token, profiling_mode, benchmark_id) {
        Ok(Some(comment_id)) => {
            println!(
                "Found existing comment (id: {}) for profiling mode: {}",
                comment_id, profiling_mode
            );
            update_comment(repo, comment_id, token, body)
        }
        Ok(None) => {
            println!("No existing comment found, creating new comment");
            create_comment(repo, pr_number, token, body)
        }
        Err(e) => {
            println!("Error searching for existing comment: {}", e);
            println!("Falling back to creating new comment");
            create_comment(repo, pr_number, token, body)
        }
    }
}
