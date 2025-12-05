use std::time::Duration;

#[hotpath::measure]
fn sync_function(sleep: u64) {
    let vec1 = vec![
        1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ];
    std::hint::black_box(&vec1);
    drop(vec1);
    let vec2 = vec![
        1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ];
    std::hint::black_box(&vec2);
    std::thread::sleep(Duration::from_nanos(sleep));
}

#[hotpath::measure]
async fn async_function(sleep: u64) {
    let vec1 = vec![1, 2, 3, 5, 6, 7, 8, 9, 10];
    std::hint::black_box(&vec1);
    drop(vec1);
    let vec = vec![1, 2, 3, 5, 6, 7, 8, 9, 10];
    std::hint::black_box(&vec);
    tokio::time::sleep(Duration::from_nanos(sleep)).await;
}

use hotpath::Reporter;

#[allow(unused)]
struct FileReporter;

impl Reporter for FileReporter {
    fn report(
        &self,
        metrics_provider: &dyn hotpath::MetricsProvider<'_>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut output = String::new();
        output.push_str(&format!(
            "HotPath Report for: {}\n",
            metrics_provider.caller_name()
        ));
        output.push_str(&format!(
            "Description: {}\n",
            metrics_provider.description()
        ));

        let metric_data = metrics_provider.metric_data();
        output.push_str(&format!("Functions measured: {}\n", metric_data.len()));
        output.push_str(&metrics_provider.headers().join(", "));
        output.push('\n');

        for (function_name, metrics) in metric_data {
            output.push_str(&format!(
                "{}, {}\n",
                function_name,
                metrics
                    .into_iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ));
        }

        std::fs::write("hotpath_report.csv", output)?;
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _hotpath = hotpath::FunctionsGuardBuilder::new("main")
        .percentiles(&[50, 90, 95])
        .reporter(Box::new(FileReporter))
        .build();

    for i in 0..100 {
        sync_function(i);
        async_function(i * 2).await;

        hotpath::measure_block!("custom_block", {
            if i == 0 {
                println!("custom_block output");
            }
            std::thread::sleep(Duration::from_nanos(i * 3))
        });
    }

    Ok(())
}
