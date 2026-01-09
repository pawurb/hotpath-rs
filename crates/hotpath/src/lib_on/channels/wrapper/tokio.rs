use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

use crate::channels::wrapper::common::{register_channel, Instant, RegisteredChannel};
use crate::channels::{ChannelEvent, ChannelType, RT};

/// Internal implementation for wrapping bounded Tokio channels with optional logging.
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
    let (inner_tx, mut inner_rx) = inner;
    let capacity = inner_tx.capacity();
    let (proxy_tx, proxy_rx) = mpsc::channel::<T>(1);

    let RegisteredChannel { id, stats_tx } =
        register_channel::<T>(source, label, ChannelType::Bounded(capacity));

    // Single forwarder: inner_rx -> proxy_tx
    RT.spawn(async move {
        loop {
            tokio::select! {
                msg = inner_rx.recv() => {
                    match msg {
                        Some(msg) => {
                            let log = log_on_send(&msg);
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
                        None => break, // inner_tx dropped (all senders gone)
                    }
                }
                _ = proxy_tx.closed() => {
                    // proxy_rx was dropped, close the channel
                    break;
                }
            }
        }
        let _ = stats_tx.send(ChannelEvent::Closed { id });
    });

    (inner_tx, proxy_rx)
}

/// Wrap the inner channel with proxy ends. Returns (outer_tx, outer_rx).
/// All messages pass through the two forwarders.
pub(crate) fn wrap_channel<T: Send + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (Sender<T>, Receiver<T>) {
    wrap_channel_impl(inner, source, label, |_| None)
}

/// Wrap a bounded Tokio channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_channel_log<T: Send + std::fmt::Debug + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (Sender<T>, Receiver<T>) {
    wrap_channel_impl(inner, source, label, |msg| Some(format!("{:?}", msg)))
}

/// Internal implementation for wrapping unbounded Tokio channels with optional logging.
/// Uses single proxy design: User -> [Original] -> Thread -> [Proxy unbounded] -> User
fn wrap_unbounded_impl<T, F>(
    inner: (UnboundedSender<T>, UnboundedReceiver<T>),
    source: &'static str,
    label: Option<String>,
    mut log_on_send: F,
) -> (UnboundedSender<T>, UnboundedReceiver<T>)
where
    T: Send + 'static,
    F: FnMut(&T) -> Option<String> + Send + 'static,
{
    let (inner_tx, mut inner_rx) = inner;
    let (proxy_tx, proxy_rx) = mpsc::unbounded_channel::<T>();

    let RegisteredChannel { id, stats_tx } =
        register_channel::<T>(source, label, ChannelType::Unbounded);

    // Single forwarder: inner_rx -> proxy_tx
    RT.spawn(async move {
        loop {
            tokio::select! {
                msg = inner_rx.recv() => {
                    match msg {
                        Some(msg) => {
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
                        None => break, // inner_tx dropped (all senders gone)
                    }
                }
                _ = proxy_tx.closed() => {
                    // proxy_rx was dropped, close the channel
                    break;
                }
            }
        }
        let _ = stats_tx.send(ChannelEvent::Closed { id });
    });

    (inner_tx, proxy_rx)
}

/// Wrap an unbounded channel with proxy ends. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_unbounded<T: Send + 'static>(
    inner: (UnboundedSender<T>, UnboundedReceiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    wrap_unbounded_impl(inner, source, label, |_| None)
}

/// Wrap an unbounded Tokio channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_unbounded_log<T: Send + std::fmt::Debug + 'static>(
    inner: (UnboundedSender<T>, UnboundedReceiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    wrap_unbounded_impl(inner, source, label, |msg| Some(format!("{:?}", msg)))
}

/// Internal implementation for wrapping oneshot Tokio channels with optional logging.
fn wrap_oneshot_impl<T, F>(
    inner: (oneshot::Sender<T>, oneshot::Receiver<T>),
    source: &'static str,
    label: Option<String>,
    mut log_on_send: F,
) -> (oneshot::Sender<T>, oneshot::Receiver<T>)
where
    T: Send + 'static,
    F: FnMut(&T) -> Option<String> + Send + 'static,
{
    let (inner_tx, inner_rx) = inner;
    let (proxy_tx, proxy_rx) = oneshot::channel::<T>();

    let RegisteredChannel { id, stats_tx } =
        register_channel::<T>(source, label, ChannelType::Oneshot);

    // Single forwarder: inner_rx -> proxy_tx
    RT.spawn(async move {
        let mut inner_rx = Some(inner_rx);
        let mut proxy_tx = Some(proxy_tx);
        let mut message_completed = false;

        tokio::select! {
            msg = async { inner_rx.take().unwrap().await }, if inner_rx.is_some() => {
                match msg {
                    Ok(msg) => {
                        let log = log_on_send(&msg);
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
            _ = async { proxy_tx.as_mut().unwrap().closed().await }, if proxy_tx.is_some() => {
                // proxy_rx was dropped - drop inner_rx to make inner_tx.send() fail
                drop(inner_rx);
            }
        }

        if !message_completed {
            let _ = stats_tx.send(ChannelEvent::Closed { id });
        }
    });

    (inner_tx, proxy_rx)
}

/// Wrap a oneshot channel with proxy ends. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_oneshot<T: Send + 'static>(
    inner: (oneshot::Sender<T>, oneshot::Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (oneshot::Sender<T>, oneshot::Receiver<T>) {
    wrap_oneshot_impl(inner, source, label, |_| None)
}

/// Wrap a oneshot Tokio channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_oneshot_log<T: Send + std::fmt::Debug + 'static>(
    inner: (oneshot::Sender<T>, oneshot::Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (oneshot::Sender<T>, oneshot::Receiver<T>) {
    wrap_oneshot_impl(inner, source, label, |msg| Some(format!("{:?}", msg)))
}

use crate::channels::InstrumentChannel;

impl<T: Send + 'static> InstrumentChannel for (Sender<T>, Receiver<T>) {
    type Output = (Sender<T>, Receiver<T>);
    fn instrument(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        wrap_channel(self, source, label)
    }
}

impl<T: Send + 'static> InstrumentChannel for (UnboundedSender<T>, UnboundedReceiver<T>) {
    type Output = (UnboundedSender<T>, UnboundedReceiver<T>);
    fn instrument(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        wrap_unbounded(self, source, label)
    }
}

impl<T: Send + 'static> InstrumentChannel for (oneshot::Sender<T>, oneshot::Receiver<T>) {
    type Output = (oneshot::Sender<T>, oneshot::Receiver<T>);
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

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelLog for (Sender<T>, Receiver<T>) {
    type Output = (Sender<T>, Receiver<T>);
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
    for (UnboundedSender<T>, UnboundedReceiver<T>)
{
    type Output = (UnboundedSender<T>, UnboundedReceiver<T>);
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
    for (oneshot::Sender<T>, oneshot::Receiver<T>)
{
    type Output = (oneshot::Sender<T>, oneshot::Receiver<T>);
    fn instrument_log(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        wrap_oneshot_log(self, source, label)
    }
}
