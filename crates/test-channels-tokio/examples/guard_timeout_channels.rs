use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    hotpath::HotpathGuardBuilder::new("guard_timeout_channels")
        .sections(vec![hotpath::Section::Channels])
        .build_with_shutdown(Duration::from_secs(1));

    let (tx, mut rx) = hotpath::channel!(
        tokio::sync::mpsc::channel::<u64>(32),
        label = "timeout-channel"
    );

    let mut i = 0_u64;
    loop {
        tx.send(i).await.expect("send should succeed");
        let _ = rx.recv().await.expect("receive should succeed");
        i = i.wrapping_add(1);
        tokio::task::yield_now().await;
    }
}
