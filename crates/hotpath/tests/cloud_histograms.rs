#[cfg(all(test, feature = "hotpath"))]
mod tests {
    use std::process::{Command, Output};

    use base64::Engine;
    use hdrhistogram::serialization::Deserializer;
    use hdrhistogram::Histogram;
    use hotpath::json::{JsonFunctionsList, JsonReport};

    // cargo run -p test-all-features --example basic_all_features --features hotpath,hotpath-cloud
    fn run_example(upload: bool) -> Output {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-all-features",
            "--example",
            "basic_all_features",
            "--features",
            "hotpath,hotpath-cloud",
        ])
        .env("HOTPATH_REPORT", "functions-timing")
        .env("HOTPATH_OUTPUT_FORMAT", "json")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN");
        if upload {
            cmd.env("HOTPATH_UPLOAD", "1");
        } else {
            cmd.env_remove("HOTPATH_UPLOAD");
        }
        let output = cmd.output().expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Process did not exit successfully.\n\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn parse_timing(stdout: &str) -> JsonFunctionsList {
        let json_start = stdout.find('{').expect("No JSON report in output");
        let report: JsonReport = serde_json::Deserializer::from_str(&stdout[json_start..])
            .into_iter::<JsonReport>()
            .next()
            .expect("No JSON value in output")
            .expect("Failed to parse JSON report");
        report
            .functions_timing
            .expect("No functions_timing section in report")
    }

    #[test]
    fn histograms_attached_when_upload_enabled() {
        let output = run_example(true);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("upload skipped: not in GitHub Actions"),
            "Expected upload skip message, got:\n{stderr}"
        );

        let list = parse_timing(&String::from_utf8_lossy(&output.stdout));
        let entry = list
            .data
            .iter()
            .find(|e| e.name.ends_with("sync_function"))
            .expect("sync_function missing from report");
        let b64 = entry
            .histogram
            .as_ref()
            .expect("histogram missing from sync_function entry");

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("histogram is not valid base64");
        let hist: Histogram<u64> = Deserializer::new()
            .deserialize(&mut &bytes[..])
            .expect("histogram is not a valid HdrHistogram payload");
        assert_eq!(hist.len(), entry.sampled_calls);
        assert!(hist.max() > 0);

        for entry in &list.data {
            assert_eq!(
                entry.histogram.is_some(),
                entry.sampled_calls > 0,
                "histogram presence mismatch for {}",
                entry.name
            );
        }
    }

    #[test]
    fn histograms_absent_without_upload() {
        let output = run_example(false);
        let list = parse_timing(&String::from_utf8_lossy(&output.stdout));
        assert!(!list.data.is_empty());
        assert!(list.data.iter().all(|e| e.histogram.is_none()));
    }
}
