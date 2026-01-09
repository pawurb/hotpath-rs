use std::sync::mpsc::{self, Receiver, Sender, SyncSender};

use crate::channels::wrapper::common::{register_channel, Instant, RegisteredChannel};
use crate::channels::{ChannelEvent, ChannelType};

/// Internal implementation for wrapping bounded std channels with optional logging.
fn wrap_sync_channel_impl<T, F>(
    inner: (SyncSender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
    mut log_on_send: F,
) -> (SyncSender<T>, Receiver<T>)
where
    T: Send + 'static,
    F: FnMut(&T) -> Option<String> + Send + 'static,
{
    let (inner_tx, inner_rx) = inner;
    let (proxy_tx, proxy_rx) = mpsc::sync_channel::<T>(1);

    let RegisteredChannel { id, stats_tx } =
        register_channel::<T>(source, label, ChannelType::Bounded(capacity));

    // Single forwarder: inner_rx -> proxy_tx
    std::thread::spawn(move || {
        while let Ok(msg) = inner_rx.recv() {
            let log = log_on_send(&msg);
            let _ = stats_tx.send(ChannelEvent::MessageSent {
                id,
                log,
                timestamp: Instant::now(),
            });
            if proxy_tx.send(msg).is_ok() {
                let _ = stats_tx.send(ChannelEvent::MessageReceived {
                    id,
                    timestamp: Instant::now(),
                });
            } else {
                // proxy_rx dropped
                break;
            }
        }
        let _ = stats_tx.send(ChannelEvent::Closed { id });
    });

    (inner_tx, proxy_rx)
}

/// Wrap a bounded std channel with proxy ends. Returns (outer_tx, outer_rx).
/// All messages pass through the two forwarders running in separate threads.
pub(crate) fn wrap_sync_channel<T: Send + 'static>(
    inner: (SyncSender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
) -> (SyncSender<T>, Receiver<T>) {
    wrap_sync_channel_impl(inner, source, label, capacity, |_| None)
}

/// Wrap a bounded std channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_sync_channel_log<T: Send + std::fmt::Debug + 'static>(
    inner: (SyncSender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
) -> (SyncSender<T>, Receiver<T>) {
    wrap_sync_channel_impl(inner, source, label, capacity, |msg| {
        Some(format!("{:?}", msg))
    })
}

/// Internal implementation for wrapping unbounded std channels with optional logging.
/// Uses single proxy design: User -> [Original] -> Thread -> [Proxy unbounded] -> User
fn wrap_channel_impl<T, F>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    mut log_on_send: F,
) -> (Sender<T>, Receiver<T>)
where
    T: Send + 'static,
    F: FnMut(&T) -> Option<String> + Send + 'static,
{
    let (inner_tx, inner_rx) = inner;
    let (proxy_tx, proxy_rx) = mpsc::channel::<T>();

    let RegisteredChannel { id, stats_tx } =
        register_channel::<T>(source, label, ChannelType::Unbounded);

    // Single forwarder: inner_rx -> proxy_tx
    std::thread::spawn(move || {
        while let Ok(msg) = inner_rx.recv() {
            let log = log_on_send(&msg);
            let _ = stats_tx.send(ChannelEvent::MessageSent {
                id,
                log,
                timestamp: Instant::now(),
            });
            if proxy_tx.send(msg).is_ok() {
                let _ = stats_tx.send(ChannelEvent::MessageReceived {
                    id,
                    timestamp: Instant::now(),
                });
            } else {
                // proxy_rx dropped
                break;
            }
        }
        let _ = stats_tx.send(ChannelEvent::Closed { id });
    });

    (inner_tx, proxy_rx)
}

/// Wrap an unbounded std channel with proxy ends. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_channel<T: Send + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (Sender<T>, Receiver<T>) {
    wrap_channel_impl(inner, source, label, |_| None)
}

/// Wrap an unbounded std channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_channel_log<T: Send + std::fmt::Debug + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (Sender<T>, Receiver<T>) {
    wrap_channel_impl(inner, source, label, |msg| Some(format!("{:?}", msg)))
}

use crate::channels::InstrumentChannel;

impl<T: Send + 'static> InstrumentChannel
    for (std::sync::mpsc::Sender<T>, std::sync::mpsc::Receiver<T>)
{
    type Output = (std::sync::mpsc::Sender<T>, std::sync::mpsc::Receiver<T>);
    fn instrument(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        wrap_channel(self, source, label)
    }
}

impl<T: Send + 'static> InstrumentChannel
    for (std::sync::mpsc::SyncSender<T>, std::sync::mpsc::Receiver<T>)
{
    type Output = (std::sync::mpsc::SyncSender<T>, std::sync::mpsc::Receiver<T>);
    fn instrument(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
    ) -> Self::Output {
        if capacity.is_none() {
            panic!("Capacity is required for bounded std channels, because they don't expose their capacity in a public API");
        }
        wrap_sync_channel(self, source, label, capacity.unwrap())
    }
}

use crate::channels::InstrumentChannelLog;

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelLog
    for (std::sync::mpsc::Sender<T>, std::sync::mpsc::Receiver<T>)
{
    type Output = (std::sync::mpsc::Sender<T>, std::sync::mpsc::Receiver<T>);
    fn instrument_log(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        wrap_channel_log(self, source, label)
    }
}

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelLog
    for (std::sync::mpsc::SyncSender<T>, std::sync::mpsc::Receiver<T>)
{
    type Output = (std::sync::mpsc::SyncSender<T>, std::sync::mpsc::Receiver<T>);
    fn instrument_log(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
    ) -> Self::Output {
        if capacity.is_none() {
            panic!("Capacity is required for bounded std channels, because they don't expose their capacity in a public API");
        }
        wrap_sync_channel_log(self, source, label, capacity.unwrap())
    }
}
