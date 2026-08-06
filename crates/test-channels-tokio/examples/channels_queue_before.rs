//! Demonstrates unbounded queue buildup: a producer task sends a message
//! every 5ms while the consumer needs 25ms to process one. Messages pile up
//! in the unbounded channel, which shows up as a growing `Max queue` in the
//! channels report. Compare with the `channels_queue_after` example.
//!
//! Run with:
//!   cargo run -p test-channels-tokio --example channels_queue_before --features hotpath

use std::time::Duration;

use hotpath::{HotpathGuardBuilder, Section};

// Mocks slow processing (e.g. a database insert or an API call) taking
// longer than the interval between incoming jobs.
#[hotpath::measure]
async fn process_job(_job: u64) {
    tokio::time::sleep(Duration::from_millis(25)).await;
}

#[tokio::main]
async fn main() {
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::FunctionsTiming, Section::Channels])
        .build();

    let (tx, mut rx) = hotpath::channel!(
        tokio::sync::mpsc::unbounded_channel::<u64>(),
        label = "jobs"
    );

    let producer = tokio::spawn(async move {
        for job in 0..100 {
            tx.send(job).unwrap();
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });

    let consumer = tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            process_job(job).await;
        }
    });

    producer.await.unwrap();
    consumer.await.unwrap();
}
