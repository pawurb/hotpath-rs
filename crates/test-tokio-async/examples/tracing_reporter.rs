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
use tracing::{info, info_span};

struct TracingReporter;

impl Reporter for TracingReporter {
    fn report(
        &self,
        metrics_provider: &dyn hotpath::MetricsProvider<'_>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("HotPath Report for: {}", metrics_provider.caller_name());
        info!("Headers: {}", metrics_provider.headers().join(", "));

        let metric_data = metrics_provider.metric_data();
        for (function_name, metrics) in metric_data {
            let func_span = info_span!("metrics", function = %function_name);
            let _f_enter = func_span.enter();

            info!(
                "{}, {}",
                function_name,
                metrics
                    .into_iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            );
        }

        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let _hotpath = hotpath::FunctionsGuardBuilder::new("main")
        .percentiles(&[50, 90, 95])
        .reporter(Box::new(TracingReporter))
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
