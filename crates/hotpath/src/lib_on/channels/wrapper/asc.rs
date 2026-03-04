use async_channel::{Receiver, Sender};

use crate::channels::{
    register_channel, ChannelEvent, ChannelType, Instant, RegisteredChannel, RT,
};

/// Internal implementation for wrapping bounded async channels with optional logging.
fn wrap_bounded_impl<T, F>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
    mut log_on_send: F,
) -> (Sender<T>, Receiver<T>)
where
    T: Send + 'static,
    F: FnMut(&T) -> Option<String> + Send + 'static,
{
    let (inner_tx, inner_rx) = inner;
    let (proxy_tx, proxy_rx) = async_channel::bounded::<T>(1);

    let RegisteredChannel { id, stats_tx } =
        register_channel::<T>(source, label, ChannelType::Bounded(capacity));

    // Single forwarder: inner_rx -> proxy_tx
    RT.spawn(async move {
        loop {
            tokio::select! {
                msg = inner_rx.recv() => {
                    match msg {
                        Ok(msg) => {
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
                        Err(_) => break, // inner_tx dropped (all senders gone)
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

    // User sends to inner_tx directly, receives from proxy_rx
    (inner_tx, proxy_rx)
}

/// Wrap the inner async channel with proxy ends. Returns (outer_tx, outer_rx).
/// All messages pass through the two forwarders.
pub(crate) fn wrap_bounded<T: Send + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
) -> (Sender<T>, Receiver<T>) {
    wrap_bounded_impl(inner, source, label, capacity, |_| None)
}

/// Wrap a bounded async channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_bounded_log<T: Send + std::fmt::Debug + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
) -> (Sender<T>, Receiver<T>) {
    wrap_bounded_impl(inner, source, label, capacity, |msg| {
        Some(crate::output::format_debug_truncated(msg))
    })
}

/// Internal implementation for wrapping unbounded async channels with optional logging.
/// Uses single proxy design: User -> [Original] -> Thread -> [Proxy unbounded] -> User
fn wrap_unbounded_impl<T, F>(
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
    let (proxy_tx, proxy_rx) = async_channel::unbounded::<T>();

    let RegisteredChannel { id, stats_tx } =
        register_channel::<T>(source, label, ChannelType::Unbounded);

    // Single forwarder: inner_rx -> proxy_tx
    RT.spawn(async move {
        loop {
            tokio::select! {
                msg = inner_rx.recv() => {
                    match msg {
                        Ok(msg) => {
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
                        Err(_) => break, // inner_tx dropped (all senders gone)
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

/// Wrap an unbounded async channel with proxy ends. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_unbounded<T: Send + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (Sender<T>, Receiver<T>) {
    wrap_unbounded_impl(inner, source, label, |_| None)
}

/// Wrap an unbounded async channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_unbounded_log<T: Send + std::fmt::Debug + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (Sender<T>, Receiver<T>) {
    wrap_unbounded_impl(inner, source, label, |msg| {
        Some(crate::output::format_debug_truncated(msg))
    })
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
        // async-channel uses the same Sender/Receiver types for both bounded and unbounded
        // We check the capacity to determine which type it is
        match self.0.capacity() {
            Some(capacity) => wrap_bounded(self, source, label, capacity),
            None => wrap_unbounded(self, source, label),
        }
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
        match self.0.capacity() {
            Some(capacity) => wrap_bounded_log(self, source, label, capacity),
            None => wrap_unbounded_log(self, source, label),
        }
    }
}
