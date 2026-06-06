use flume::{Receiver, Sender};

use crate::channels::{
    register_channel, send_channel_event, ChannelEvent, ChannelType, Instant, RT,
};

/// Internal implementation for wrapping bounded flume channels with optional logging.
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
    let (proxy_tx, proxy_rx) = flume::bounded::<T>(1);

    let id = register_channel::<T>(source, label, ChannelType::Bounded(capacity));

    // Single forwarder: inner_rx -> proxy_tx
    RT.spawn(async move {
        while let Ok(msg) = inner_rx.recv_async().await {
            let log = log_on_send(&msg);
            send_channel_event(ChannelEvent::MessageSent {
                id,
                log,
                timestamp: Instant::now(),
            });
            if proxy_tx.send_async(msg).await.is_ok() {
                send_channel_event(ChannelEvent::MessageReceived {
                    id,
                    timestamp: Instant::now(),
                });
            } else {
                // proxy_rx dropped
                break;
            }
        }
        send_channel_event(ChannelEvent::Closed { id });
    });

    // User sends to inner_tx directly, receives from proxy_rx
    (inner_tx, proxy_rx)
}

/// Wrap the inner flume channel with proxy ends. Returns (outer_tx, outer_rx).
/// All messages pass through a single forwarder task.
pub(crate) fn wrap_bounded<T: Send + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
) -> (Sender<T>, Receiver<T>) {
    wrap_bounded_impl(inner, source, label, capacity, |_| None)
}

/// Wrap a bounded flume channel with logging enabled. Returns (outer_tx, outer_rx).
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

/// Internal implementation for wrapping unbounded flume channels with optional logging.
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
    let (proxy_tx, proxy_rx) = flume::unbounded::<T>();

    let id = register_channel::<T>(source, label, ChannelType::Unbounded);

    // Single forwarder: inner_rx -> proxy_tx
    RT.spawn(async move {
        while let Ok(msg) = inner_rx.recv_async().await {
            let log = log_on_send(&msg);
            send_channel_event(ChannelEvent::MessageSent {
                id,
                log,
                timestamp: Instant::now(),
            });
            if proxy_tx.send_async(msg).await.is_ok() {
                send_channel_event(ChannelEvent::MessageReceived {
                    id,
                    timestamp: Instant::now(),
                });
            } else {
                // proxy_rx dropped
                break;
            }
        }
        send_channel_event(ChannelEvent::Closed { id });
    });

    (inner_tx, proxy_rx)
}

/// Wrap an unbounded flume channel with proxy ends. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_unbounded<T: Send + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (Sender<T>, Receiver<T>) {
    wrap_unbounded_impl(inner, source, label, |_| None)
}

/// Wrap an unbounded flume channel with logging enabled. Returns (outer_tx, outer_rx).
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
