use crossbeam_channel::{self, Receiver, Sender};
use std::mem;
use std::sync::atomic::Ordering;

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

    let (outer_tx, to_inner_rx) = crossbeam_channel::bounded::<T>(capacity);
    let (from_inner_tx, outer_rx) = crossbeam_channel::bounded::<T>(capacity);

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
    let (close_signal_tx, close_signal_rx) = crossbeam_channel::bounded::<()>(1);

    // Forward outer -> inner (proxy the send path)
    std::thread::spawn(move || {
        loop {
            // Check for close signal (non-blocking)
            match close_signal_rx.try_recv() {
                Ok(_) => {
                    // Outer receiver was closed/dropped
                    break;
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    // Close signal sender dropped, which means recv forwarder ended
                    break;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    // No close signal, continue
                }
            }

            // Try to receive with timeout to periodically check close signal
            match to_inner_rx.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(msg) => {
                    let log = log_on_send(&msg);
                    if inner_tx.send(msg).is_err() {
                        // Inner receiver dropped
                        break;
                    }
                    let _ = stats_tx_send.send(ChannelEvent::MessageSent {
                        id,
                        log,
                        timestamp: std::time::Instant::now(),
                    });
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // No message, loop again to check close signal
                    continue;
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    // Outer sender dropped
                    break;
                }
            }
        }
        // Channel is closed
        let _ = stats_tx_send.send(ChannelEvent::Closed { id });
    });

    // Forward inner -> outer (proxy the recv path)
    std::thread::spawn(move || {
        while let Ok(msg) = inner_rx.recv() {
            if from_inner_tx.send(msg).is_err() {
                // Outer receiver was closed
                let _ = close_signal_tx.send(());
                break;
            }
            let _ = stats_tx_recv.send(ChannelEvent::MessageReceived {
                id,
                timestamp: std::time::Instant::now(),
            });
        }
        // Channel is closed (either inner sender dropped or outer receiver closed)
        let _ = stats_tx_recv.send(ChannelEvent::Closed { id });
    });

    (outer_tx, outer_rx)
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

    let (outer_tx, to_inner_rx) = crossbeam_channel::unbounded::<T>();
    let (from_inner_tx, outer_rx) = crossbeam_channel::unbounded::<T>();

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
    let (close_signal_tx, close_signal_rx) = crossbeam_channel::bounded::<()>(1);

    // Forward outer -> inner (proxy the send path)
    std::thread::spawn(move || {
        loop {
            // Check for close signal (non-blocking)
            match close_signal_rx.try_recv() {
                Ok(_) => {
                    // Outer receiver was closed/dropped
                    break;
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    // Close signal sender dropped, which means recv forwarder ended
                    break;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    // No close signal, continue
                }
            }

            // Try to receive with timeout to periodically check close signal
            match to_inner_rx.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(msg) => {
                    let log = log_on_send(&msg);
                    if inner_tx.send(msg).is_err() {
                        // Inner receiver dropped
                        break;
                    }
                    let _ = stats_tx_send.send(ChannelEvent::MessageSent {
                        id,
                        log,
                        timestamp: std::time::Instant::now(),
                    });
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // No message, loop again to check close signal
                    continue;
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    // Outer sender dropped
                    break;
                }
            }
        }
        // Channel is closed
        let _ = stats_tx_send.send(ChannelEvent::Closed { id });
    });

    // Forward inner -> outer (proxy the recv path)
    std::thread::spawn(move || {
        while let Ok(msg) = inner_rx.recv() {
            if from_inner_tx.send(msg).is_err() {
                // Outer receiver was closed
                let _ = close_signal_tx.send(());
                break;
            }
            let _ = stats_tx_recv.send(ChannelEvent::MessageReceived {
                id,
                timestamp: std::time::Instant::now(),
            });
        }
        // Channel is closed (either inner sender dropped or outer receiver closed)
        let _ = stats_tx_recv.send(ChannelEvent::Closed { id });
    });

    (outer_tx, outer_rx)
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

use crate::channels::Instrument;

impl<T: Send + 'static> Instrument
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

use crate::channels::InstrumentLog;

impl<T: Send + std::fmt::Debug + 'static> InstrumentLog
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
