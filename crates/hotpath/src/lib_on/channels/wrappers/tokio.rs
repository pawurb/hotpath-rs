use std::mem;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

use crate::channels::RT;
use crate::channels::{init_channels_state, ChannelEvent, ChannelType, CHANNEL_ID_COUNTER};

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
    let type_name = std::any::type_name::<T>();

    let capacity = inner_tx.capacity();
    let (outer_tx, mut to_inner_rx) = mpsc::channel::<T>(capacity);
    let (from_inner_tx, outer_rx) = mpsc::channel::<T>(capacity);

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

    let stats_tx_send = stats_tx.clone();
    let stats_tx_recv = stats_tx.clone();

    // Create a signal channel to notify send-forwarder when outer_rx is closed
    let (close_signal_tx, mut close_signal_rx) = oneshot::channel::<()>();

    // Forward outer -> inner (proxy the send path)
    RT.spawn(async move {
        loop {
            tokio::select! {
                msg = to_inner_rx.recv() => {
                    match msg {
                        Some(msg) => {
                            let log = log_on_send(&msg);
                            if inner_tx.send(msg).await.is_err() {
                                to_inner_rx.close();
                                break;
                            }
                            let _ = stats_tx_send.send(ChannelEvent::MessageSent {
                                id,
                                log,
                                timestamp: std::time::Instant::now(),
                            });
                        }
                        None => break, // Outer sender dropped
                    }
                }
                _ = &mut close_signal_rx => {
                    // Outer receiver was closed/dropped, close our receiver to reject further sends
                    to_inner_rx.close();
                    break;
                }
            }
        }
        // Channel is closed
        let _ = stats_tx_send.send(ChannelEvent::Closed { id });
    });

    // Forward inner -> outer (proxy the recv path)
    RT.spawn(async move {
        loop {
            tokio::select! {
                msg = inner_rx.recv() => {
                    match msg {
                        Some(msg) => {
                            if from_inner_tx.send(msg).await.is_ok() {
                                let _ = stats_tx_recv.send(ChannelEvent::MessageReceived {
                                    id,
                                    timestamp: std::time::Instant::now(),
                                });
                            } else {
                                let _ = close_signal_tx.send(());
                                break;
                            }
                        }
                        None => break, // Inner sender dropped
                    }
                }
                _ = from_inner_tx.closed() => {
                    // Outer receiver was closed/dropped
                    let _ = close_signal_tx.send(());
                    break;
                }
            }
        }
        // Channel is closed (either inner sender dropped or outer receiver closed)
        let _ = stats_tx_recv.send(ChannelEvent::Closed { id });
    });

    (outer_tx, outer_rx)
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
    let type_name = std::any::type_name::<T>();

    let (outer_tx, mut to_inner_rx) = mpsc::unbounded_channel::<T>();
    let (from_inner_tx, outer_rx) = mpsc::unbounded_channel::<T>();

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

    let stats_tx_send = stats_tx.clone();
    let stats_tx_recv = stats_tx.clone();

    // Create a signal channel to notify send-forwarder when outer_rx is closed
    let (close_signal_tx, mut close_signal_rx) = oneshot::channel::<()>();

    // Forward outer -> inner (proxy the send path)
    RT.spawn(async move {
        loop {
            tokio::select! {
                msg = to_inner_rx.recv() => {
                    match msg {
                        Some(msg) => {
                            let log = log_on_send(&msg);
                            if inner_tx.send(msg).is_err() {
                                to_inner_rx.close();
                                break;
                            }
                            let _ = stats_tx_send.send(ChannelEvent::MessageSent {
                                id,
                                log,
                                timestamp: std::time::Instant::now(),
                            });
                        }
                        None => break, // Outer sender dropped
                    }
                }
                _ = &mut close_signal_rx => {
                    // Outer receiver was closed/dropped, close our receiver to reject further sends
                    to_inner_rx.close();
                    break;
                }
            }
        }
        // Channel is closed
        let _ = stats_tx_send.send(ChannelEvent::Closed { id });
    });

    // Forward inner -> outer (proxy the recv path)
    RT.spawn(async move {
        loop {
            tokio::select! {
                msg = inner_rx.recv() => {
                    match msg {
                        Some(msg) => {
                            if from_inner_tx.send(msg).is_ok() {
                                let _ = stats_tx_recv.send(ChannelEvent::MessageReceived {
                                    id,
                                    timestamp: std::time::Instant::now(),
                                });
                            } else {
                                // Outer receiver was closed
                                let _ = close_signal_tx.send(());
                                break;
                            }
                        }
                        None => break, // Inner sender dropped
                    }
                }
                _ = from_inner_tx.closed() => {
                    // Outer receiver was closed/dropped
                    let _ = close_signal_tx.send(());
                    break;
                }
            }
        }
        // Channel is closed (either inner sender dropped or outer receiver closed)
        let _ = stats_tx_recv.send(ChannelEvent::Closed { id });
    });

    (outer_tx, outer_rx)
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
    let type_name = std::any::type_name::<T>();

    let (outer_tx, outer_rx_proxy) = oneshot::channel::<T>();
    let (mut inner_tx_proxy, outer_rx) = oneshot::channel::<T>();

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

    let stats_tx_send = stats_tx.clone();
    let stats_tx_recv = stats_tx;

    // Create a signal channel to notify send-forwarder when outer_rx is closed
    let (close_signal_tx, mut close_signal_rx) = oneshot::channel::<()>();

    // Monitor outer receiver and drop inner receiver when outer is dropped
    RT.spawn(async move {
        let mut inner_rx = Some(inner_rx);
        let mut message_received = false;
        tokio::select! {
            msg = async { inner_rx.take().unwrap().await }, if inner_rx.is_some() => {
                // Message received from inner
                match msg {
                    Ok(msg) => {
                        if inner_tx_proxy.send(msg).is_ok() {
                            let _ = stats_tx_recv.send(ChannelEvent::MessageReceived {
                                id,
                                timestamp: std::time::Instant::now(),
                            });
                            message_received = true;
                        }
                    }
                    Err(_) => {
                        // Inner sender was dropped without sending
                    }
                }
            }
            _ = inner_tx_proxy.closed() => {
                // Outer receiver was dropped - drop inner_rx to make sends fail
                drop(inner_rx);
                let _ = close_signal_tx.send(());
            }
        }
        // Only send Closed if message was not successfully received
        if !message_received {
            let _ = stats_tx_recv.send(ChannelEvent::Closed { id });
        }
    });

    // Forward outer -> inner (proxy the send path)
    RT.spawn(async move {
        let mut message_sent = false;
        tokio::select! {
            msg = outer_rx_proxy => {
                match msg {
                    Ok(msg) => {
                        let log = log_on_send(&msg);
                        if inner_tx.send(msg).is_ok() {
                            let _ = stats_tx_send.send(ChannelEvent::MessageSent {
                                id,
                                log,
                                timestamp: std::time::Instant::now(),
                            });
                            let _ = stats_tx_send.send(ChannelEvent::Notified { id });
                            message_sent = true;
                        }
                    }
                    Err(_) => {
                        // Outer sender was dropped without sending
                    }
                }
            }
            _ = &mut close_signal_rx => {
                // Outer receiver was closed/dropped before send
            }
        }
        // Only send Closed if message was not successfully sent
        if !message_sent {
            let _ = stats_tx_send.send(ChannelEvent::Closed { id });
        }
    });

    (outer_tx, outer_rx)
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

use crate::channels::Instrument;

impl<T: Send + 'static> Instrument for (Sender<T>, Receiver<T>) {
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

impl<T: Send + 'static> Instrument for (UnboundedSender<T>, UnboundedReceiver<T>) {
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

impl<T: Send + 'static> Instrument for (oneshot::Sender<T>, oneshot::Receiver<T>) {
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

use crate::channels::InstrumentLog;

impl<T: Send + std::fmt::Debug + 'static> InstrumentLog for (Sender<T>, Receiver<T>) {
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

impl<T: Send + std::fmt::Debug + 'static> InstrumentLog
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

impl<T: Send + std::fmt::Debug + 'static> InstrumentLog
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
