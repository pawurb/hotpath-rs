use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::common::assert_contains_all;
use crate::common::example::{poll_endpoint, Example};

/// Runs `example` with the metrics server on `port`, waits for `/{route}` to
/// list every `all_expected` needle, then checks that `/{route}/{id}/logs` of
/// the first entry answers 200. Returns the logs body, `None` for an empty list.
#[track_caller]
pub(crate) fn assert_list_and_logs_endpoints<L: DeserializeOwned>(
    example: Example,
    port: u16,
    route: &str,
    all_expected: &[&str],
    first_id: impl Fn(&L) -> Option<u32>,
) -> Option<String> {
    let _running = example
        .env("HOTPATH_METRICS_PORT", port.to_string())
        .env("TEST_SLEEP_SECONDS", "10")
        .spawn();

    let json_text = poll_endpoint(
        &format!("http://localhost:{port}/{route}"),
        12,
        Duration::from_millis(750),
        |body| all_expected.iter().all(|e| body.contains(e)),
    );
    assert_contains_all(&json_text, all_expected);

    let list: L = serde_json::from_str(&json_text)
        .unwrap_or_else(|e| panic!("Failed to parse /{route} JSON: {e}"));
    let id = first_id(&list)?;

    let mut response = ureq::get(&format!("http://localhost:{port}/{route}/{id}/logs"))
        .call()
        .unwrap_or_else(|e| panic!("Failed to call /{route}/{id}/logs endpoint: {e}"));
    assert_eq!(
        response.status(),
        200,
        "Expected status 200 for /{route}/{id}/logs endpoint"
    );
    Some(
        response
            .body_mut()
            .read_to_string()
            .expect("Failed to read logs response"),
    )
}
