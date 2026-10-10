use hotpath::json::JsonReport;

/// Trailing log lines follow the report, so parse only the first JSON value.
#[track_caller]
pub(crate) fn parse_report(stdout: &str) -> JsonReport {
    let json_start = stdout.find('{').expect("No JSON report in output");
    serde_json::Deserializer::from_str(&stdout[json_start..])
        .into_iter::<JsonReport>()
        .next()
        .expect("No JSON value in output")
        .expect("Failed to parse JSON report")
}

impl crate::common::example::Example {
    /// Runs to completion, asserts a successful exit and parses the JSON
    /// report from stdout.
    #[track_caller]
    pub(crate) fn report(self) -> JsonReport {
        parse_report(&self.stdout())
    }
}
