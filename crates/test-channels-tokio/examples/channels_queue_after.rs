//! Fixed version of the `channels_queue_before` example: the consumer now
//! processes a message in 5ms while the producer sends one every 25ms.
//! The consumer keeps up, so the channels report shows `Max queue` of 1 -
//! each message is picked up before the next one arrives.
//!
//! Run with:
//!   cargo run -p test-channels-tokio --example channels_queue_after --features hotpath

use std::time::Duration;

use hotpath::{HotpathGuardBuilder, Section};

// Processing is now faster than the interval between incoming jobs.
#[hotpath::measure]
async fn process_job(_job: u64) {
    tokio::time::sleep(Duration::from_millis(5)).await;
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
            tokio::time::sleep(Duration::from_millis(25)).await;
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
