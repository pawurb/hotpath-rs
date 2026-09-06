//! Reads CI provenance from the environment: the `meta.ci` object plus what
//! the environment claims about `meta.git`. Those claims hold only for a run
//! that built the commit the event names, so `report_meta::merge_git_info`
//! decides which of them survive.
//!
//! Every field is best-effort: a missing variable, an unreadable event file
//! or an unexpected payload shape omits a field, never fails the run.

use std::path::Path;

use serde::Deserialize;

use crate::json::JsonCiInfo;

/// The `meta.ci` object plus the commit identity the environment claims.
pub(crate) struct CiContext {
    pub(crate) ci: JsonCiInfo,
    pub(crate) sha: Option<String>,
    pub(crate) r#ref: Option<String>,
    pub(crate) repository: Option<String>,
    pub(crate) base_sha: Option<String>,
}

pub(crate) fn detect() -> Option<CiContext> {
    (env("GITHUB_ACTIONS").as_deref() == Some("true")).then(github_actions)
}

fn github_actions() -> CiContext {
    let event = env("GITHUB_EVENT_NAME");
    let pull_request = match event.as_deref() {
        Some("pull_request") => {
            env("GITHUB_EVENT_PATH").and_then(|path| read_pull_request(Path::new(&path)))
        }
        _ => None,
    };

    CiContext {
        ci: JsonCiInfo {
            provider: "github-actions".to_string(),
            event,
            pr_number: pull_request.as_ref().and_then(|pr| pr.number),
            run_id: env("GITHUB_RUN_ID"),
            base_ref: env("GITHUB_BASE_REF"),
            head_ref: env("GITHUB_HEAD_REF"),
            workflow: env("GITHUB_WORKFLOW"),
            actor: env("GITHUB_ACTOR"),
            repository_id: env("GITHUB_REPOSITORY_ID"),
        },
        sha: env("GITHUB_SHA"),
        r#ref: env("GITHUB_REF"),
        repository: env("GITHUB_REPOSITORY"),
        base_sha: pull_request
            .and_then(|pr| pr.base)
            .and_then(|base| base.sha),
    }
}

#[derive(Deserialize)]
struct EventPayload {
    pull_request: Option<PullRequest>,
}

#[derive(Deserialize)]
struct PullRequest {
    number: Option<u64>,
    base: Option<EventBase>,
}

#[derive(Deserialize)]
struct EventBase {
    sha: Option<String>,
}

/// The runner writes `$GITHUB_EVENT_PATH` once, when the event fired, so the
/// base sha in it and `GITHUB_SHA` describe the same snapshot by
/// construction. Asking the REST API instead would answer with the pull
/// request's base as of now, which drifts while a benchmark runs.
fn read_pull_request(path: &Path) -> Option<PullRequest> {
    let contents = std::fs::read_to_string(path).ok()?;
    let payload: EventPayload = serde_json::from_str(&contents).ok()?;
    payload.pull_request
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use crate::lib_on::ci_info::read_pull_request;
    use std::path::PathBuf;

    fn write_event(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hotpath-ci-info-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.json"));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn reads_pull_request_payload() {
        let path = write_event(
            "pull_request",
            r#"{
                "action": "synchronize",
                "number": 42,
                "pull_request": {
                    "number": 42,
                    "head": { "sha": "1111111111111111111111111111111111111111" },
                    "base": {
                        "ref": "main",
                        "sha": "2222222222222222222222222222222222222222",
                        "repo": { "full_name": "owner/name" }
                    }
                },
                "repository": { "full_name": "owner/name" }
            }"#,
        );

        let pr = read_pull_request(&path).expect("pull_request parsed");
        assert_eq!(pr.number, Some(42));
        assert_eq!(
            pr.base.and_then(|b| b.sha).as_deref(),
            Some("2222222222222222222222222222222222222222")
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn push_payload_has_no_pull_request() {
        let path = write_event(
            "push",
            r#"{"ref":"refs/heads/main","after":"3333333333333333333333333333333333333333"}"#,
        );
        assert!(read_pull_request(&path).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tolerates_unexpected_shapes() {
        let missing = std::env::temp_dir().join("hotpath-ci-info-does-not-exist.json");
        assert!(read_pull_request(&missing).is_none());

        let invalid = write_event("invalid", "not json {");
        assert!(read_pull_request(&invalid).is_none());
        let _ = std::fs::remove_file(&invalid);

        let wrong_type = write_event("wrong_type", r#"{"pull_request":{"number":"42"}}"#);
        assert!(read_pull_request(&wrong_type).is_none());
        let _ = std::fs::remove_file(&wrong_type);

        let partial = write_event("partial", r#"{"pull_request":{"base":{}}}"#);
        let pr = read_pull_request(&partial).expect("pull_request parsed");
        assert_eq!(pr.number, None);
        assert!(pr.base.expect("base parsed").sha.is_none());
        let _ = std::fs::remove_file(&partial);
    }
}
