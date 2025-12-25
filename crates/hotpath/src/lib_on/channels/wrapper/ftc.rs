use futures_channel::mpsc;
use futures_channel::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender};
use futures_channel::oneshot;
use futures_util::sink::SinkExt;
#[cfg(target_os = "linux")]
use quanta::Instant;
use std::mem;
use std::sync::atomic::Ordering;
#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use crate::channels::RT;
use crate::channels::{init_channels_state, ChannelEvent, ChannelType, CHANNEL_ID_COUNTER};

/// Internal implementation for wrapping bounded futures channels with optional logging.
fn wrap_channel_impl<T, F>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
    mut get_msg_log: F,
) -> (Sender<T>, Receiver<T>)
where
    T: Send + 'static,
    F: FnMut(&T) -> Option<String> + Send + 'static + Clone,
{
    let (inner_tx, mut inner_rx) = inner;
    let type_name = std::any::type_name::<T>();

    let (mut proxy_tx, proxy_rx) = mpsc::channel::<T>(1);

    let (stats_tx, _) = init_channels_state();

    let id = CHANNEL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

    let _ = stats_tx.send(ChannelEvent::Created {
        id,
        source,
        display_label: label,
        channel_type: ChannelType::Bounded(capacity),
        type_name,
        type_size: mem::size_of::<T>(),
    });

    // Single forwarder: inner_rx -> proxy_tx
    RT.spawn(async move {
        use futures_util::stream::StreamExt;
        while let Some(msg) = inner_rx.next().await {
            let log = get_msg_log(&msg);
            let _ = stats_tx.send(ChannelEvent::MessageSent {
                id,
                log,
                timestamp: Instant::now(),
            });
            if proxy_tx.send(msg).await.is_ok() {
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

    // User sends to inner_tx directly, receives from proxy_rx
    (inner_tx, proxy_rx)
}

/// Wrap the inner futures channel with proxy ends. Returns (outer_tx, outer_rx).
/// All messages pass through the two forwarders.
pub(crate) fn wrap_channel<T: Send + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
) -> (Sender<T>, Receiver<T>) {
    wrap_channel_impl(inner, source, label, capacity, |_| None)
}

/// Wrap a bounded futures channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_channel_log<T: Send + std::fmt::Debug + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
) -> (Sender<T>, Receiver<T>) {
    wrap_channel_impl(inner, source, label, capacity, |msg| {
        Some(format!("{:?}", msg))
    })
}

/// Internal implementation for wrapping unbounded futures channels with optional logging.
/// Uses single proxy design: User -> [Original] -> Thread -> [Proxy unbounded] -> User
fn wrap_unbounded_impl<T, F>(
    inner: (UnboundedSender<T>, UnboundedReceiver<T>),
    source: &'static str,
    label: Option<String>,
    mut get_msg_log: F,
) -> (UnboundedSender<T>, UnboundedReceiver<T>)
where
    T: Send + 'static,
    F: FnMut(&T) -> Option<String> + Send + 'static + Clone,
{
    let (inner_tx, mut inner_rx) = inner;
    let type_name = std::any::type_name::<T>();

    let (proxy_tx, proxy_rx) = mpsc::unbounded::<T>();

    let (stats_tx, _) = init_channels_state();

    let id = CHANNEL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

    let _ = stats_tx.send(ChannelEvent::Created {
        id,
        source,
        display_label: label,
        channel_type: ChannelType::Unbounded,
        type_name,
        type_size: mem::size_of::<T>(),
    });

    // Single forwarder: inner_rx -> proxy_tx
    RT.spawn(async move {
        use futures_util::stream::StreamExt;
        while let Some(msg) = inner_rx.next().await {
            let log = get_msg_log(&msg);
            let _ = stats_tx.send(ChannelEvent::MessageSent {
                id,
                log,
                timestamp: Instant::now(),
            });
            if proxy_tx.unbounded_send(msg).is_ok() {
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

    // User sends to inner_tx directly, receives from proxy_rx
    (inner_tx, proxy_rx)
}

/// Wrap an unbounded futures channel with proxy ends. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_unbounded<T: Send + 'static>(
    inner: (UnboundedSender<T>, UnboundedReceiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    wrap_unbounded_impl(inner, source, label, |_| None)
}

/// Wrap an unbounded futures channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_unbounded_log<T: Send + std::fmt::Debug + 'static>(
    inner: (UnboundedSender<T>, UnboundedReceiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    wrap_unbounded_impl(inner, source, label, |msg| Some(format!("{:?}", msg)))
}

/// Internal implementation for wrapping oneshot futures channels with optional logging.
fn wrap_oneshot_impl<T, F>(
    inner: (oneshot::Sender<T>, oneshot::Receiver<T>),
    source: &'static str,
    label: Option<String>,
    mut get_msg_log: F,
) -> (oneshot::Sender<T>, oneshot::Receiver<T>)
where
    T: Send + 'static,
    F: FnMut(&T) -> Option<String> + Send + 'static + Clone,
{
    let (inner_tx, inner_rx) = inner;
    let type_name = std::any::type_name::<T>();

    // Single proxy oneshot channel
    let (proxy_tx, proxy_rx) = oneshot::channel::<T>();

    let (stats_tx, _) = init_channels_state();

    let id = CHANNEL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

    let _ = stats_tx.send(ChannelEvent::Created {
        id,
        source,
        display_label: label,
        channel_type: ChannelType::Oneshot,
        type_name,
        type_size: mem::size_of::<T>(),
    });

    // Single forwarder: inner_rx -> proxy_tx
    RT.spawn(async move {
        let mut inner_rx = Some(inner_rx);
        let mut proxy_tx = Some(proxy_tx);
        let mut message_completed = false;

        tokio::select! {
            msg = async { inner_rx.take().unwrap().await }, if inner_rx.is_some() => {
                match msg {
                    Ok(msg) => {
                        let log = get_msg_log(&msg);
                        let _ = stats_tx.send(ChannelEvent::MessageSent {
                            id,
                            log,
                            timestamp: Instant::now(),
                        });
                        let _ = stats_tx.send(ChannelEvent::Notified { id });
                        if proxy_tx.take().unwrap().send(msg).is_ok() {
                            let _ = stats_tx.send(ChannelEvent::MessageReceived {
                                id,
                                timestamp: Instant::now(),
                            });
                            message_completed = true;
                        }
                    }
                    Err(_) => {
                        // inner_tx was dropped without sending
                    }
                }
            }
            _ = async { proxy_tx.as_mut().unwrap().cancellation().await }, if proxy_tx.is_some() => {
                // proxy_rx was dropped - drop inner_rx to make inner_tx.send() fail
                drop(inner_rx);
            }
        }

        if !message_completed {
            let _ = stats_tx.send(ChannelEvent::Closed { id });
        }
    });

    // User sends to inner_tx directly, receives from proxy_rx
    (inner_tx, proxy_rx)
}

/// Wrap a oneshot futures channel with proxy ends. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_oneshot<T: Send + 'static>(
    inner: (oneshot::Sender<T>, oneshot::Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (oneshot::Sender<T>, oneshot::Receiver<T>) {
    wrap_oneshot_impl(inner, source, label, |_| None)
}

/// Wrap a oneshot futures channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_oneshot_log<T: Send + std::fmt::Debug + 'static>(
    inner: (oneshot::Sender<T>, oneshot::Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (oneshot::Sender<T>, oneshot::Receiver<T>) {
    wrap_oneshot_impl(inner, source, label, |msg| Some(format!("{:?}", msg)))
}

use crate::channels::InstrumentChannel;

impl<T: Send + 'static> InstrumentChannel
    for (
        futures_channel::mpsc::Sender<T>,
        futures_channel::mpsc::Receiver<T>,
    )
{
    type Output = (
        futures_channel::mpsc::Sender<T>,
        futures_channel::mpsc::Receiver<T>,
    );
    fn instrument(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
    ) -> Self::Output {
        if capacity.is_none() {
            panic!("Capacity is required for bounded futures channels, because they don't expose their capacity in a public API");
        }
        wrap_channel(self, source, label, capacity.unwrap())
    }
}

impl<T: Send + 'static> InstrumentChannel
    for (
        futures_channel::mpsc::UnboundedSender<T>,
        futures_channel::mpsc::UnboundedReceiver<T>,
    )
{
    type Output = (
        futures_channel::mpsc::UnboundedSender<T>,
        futures_channel::mpsc::UnboundedReceiver<T>,
    );
    fn instrument(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        wrap_unbounded(self, source, label)
    }
}

impl<T: Send + 'static> InstrumentChannel
    for (
        futures_channel::oneshot::Sender<T>,
        futures_channel::oneshot::Receiver<T>,
    )
{
    type Output = (
        futures_channel::oneshot::Sender<T>,
        futures_channel::oneshot::Receiver<T>,
    );
    fn instrument(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        wrap_oneshot(self, source, label)
    }
}

use crate::channels::InstrumentChannelLog;

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelLog
    for (
        futures_channel::mpsc::Sender<T>,
        futures_channel::mpsc::Receiver<T>,
    )
{
    type Output = (
        futures_channel::mpsc::Sender<T>,
        futures_channel::mpsc::Receiver<T>,
    );
    fn instrument_log(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
    ) -> Self::Output {
        if capacity.is_none() {
            panic!("Capacity is required for bounded futures channels, because they don't expose their capacity in a public API");
        }
        wrap_channel_log(self, source, label, capacity.unwrap())
    }
}

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelLog
    for (
        futures_channel::mpsc::UnboundedSender<T>,
        futures_channel::mpsc::UnboundedReceiver<T>,
    )
{
    type Output = (
        futures_channel::mpsc::UnboundedSender<T>,
        futures_channel::mpsc::UnboundedReceiver<T>,
    );
    fn instrument_log(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        wrap_unbounded_log(self, source, label)
    }
}

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelLog
    for (
        futures_channel::oneshot::Sender<T>,
        futures_channel::oneshot::Receiver<T>,
    )
{
    type Output = (
        futures_channel::oneshot::Sender<T>,
        futures_channel::oneshot::Receiver<T>,
    );
    fn instrument_log(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        wrap_oneshot_log(self, source, label)
    }
}
