use crossbeam_channel::{self, Receiver, Sender};
#[cfg(target_os = "linux")]
use quanta::Instant;
use std::mem;
use std::sync::atomic::Ordering;
#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use crate::channels::{init_channels_state, ChannelEvent, ChannelType, CHANNEL_ID_COUNTER};

/// Internal implementation for wrapping bounded crossbeam channels with optional logging.
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
    let type_name = std::any::type_name::<T>();

    let (proxy_tx, proxy_rx) = crossbeam_channel::bounded::<T>(1);

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

/// Wrap a bounded crossbeam channel with proxy ends. Returns (outer_tx, outer_rx).
/// All messages pass through the two forwarders running in separate threads.
pub(crate) fn wrap_bounded<T: Send + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
) -> (Sender<T>, Receiver<T>) {
    wrap_bounded_impl(inner, source, label, capacity, |_| None)
}

/// Wrap a bounded crossbeam channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_bounded_log<T: Send + std::fmt::Debug + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
    capacity: usize,
) -> (Sender<T>, Receiver<T>) {
    wrap_bounded_impl(inner, source, label, capacity, |msg| {
        Some(format!("{:?}", msg))
    })
}

/// Internal implementation for wrapping unbounded crossbeam channels with optional logging.
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
    let type_name = std::any::type_name::<T>();

    let (proxy_tx, proxy_rx) = crossbeam_channel::unbounded::<T>();

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
    std::thread::spawn(move || {
        while let Ok(msg) = inner_rx.recv() {
            let log = log_on_send(&msg);
            let _ = stats_tx.send(ChannelEvent::MessageSent {
                id,
                log,
                timestamp: Instant::now(),
            });
            // MessageReceived logged before user receives
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

/// Wrap an unbounded crossbeam channel with proxy ends. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_unbounded<T: Send + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (Sender<T>, Receiver<T>) {
    wrap_unbounded_impl(inner, source, label, |_| None)
}

/// Wrap an unbounded crossbeam channel with logging enabled. Returns (outer_tx, outer_rx).
pub(crate) fn wrap_unbounded_log<T: Send + std::fmt::Debug + 'static>(
    inner: (Sender<T>, Receiver<T>),
    source: &'static str,
    label: Option<String>,
) -> (Sender<T>, Receiver<T>) {
    wrap_unbounded_impl(inner, source, label, |msg| Some(format!("{:?}", msg)))
}

use crate::channels::InstrumentChannel;

impl<T: Send + 'static> InstrumentChannel
    for (crossbeam_channel::Sender<T>, crossbeam_channel::Receiver<T>)
{
    type Output = (crossbeam_channel::Sender<T>, crossbeam_channel::Receiver<T>);
    fn instrument(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        // Crossbeam uses the same Sender/Receiver types for both bounded and unbounded
        // We check the capacity to determine which type it is
        match self.0.capacity() {
            Some(capacity) => wrap_bounded(self, source, label, capacity),
            None => wrap_unbounded(self, source, label),
        }
    }
}

use crate::channels::InstrumentChannelLog;

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelLog
    for (crossbeam_channel::Sender<T>, crossbeam_channel::Receiver<T>)
{
    type Output = (crossbeam_channel::Sender<T>, crossbeam_channel::Receiver<T>);
    fn instrument_log(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        // Crossbeam uses the same Sender/Receiver types for both bounded and unbounded
        // We check the capacity to determine which type it is
        match self.0.capacity() {
            Some(capacity) => wrap_bounded_log(self, source, label, capacity),
            None => wrap_unbounded_log(self, source, label),
        }
    }
}
