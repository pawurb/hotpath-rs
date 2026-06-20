//! Endpoint-wrapping crossbeam channel instrumentation (`channel!(..., wrap = true)`).
//!
//! Unlike the forwarder-proxy wrapping in [`crate::channels::wrapper::crossbeam`],
//! this wraps the `Sender`/`Receiver` endpoints directly. No extra thread or proxy
//! channel is inserted, so send/recv happen on the real channel and the queue depth
//! reported (`queue_len`) is a snapshot of the channel's length taken right after each
//! operation. It is exact for single-threaded use; with concurrent senders/receivers it
//! may skew by the number of operations other threads complete in that window.
//!
//! The inner channel carries `(msg_id, send_ts, T)` rather than `T`. The
//! monotonic `msg_id` lets a send pair with its exact matching receive even under
//! multiple producers/consumers. `send_ts` is stamped *before* the message is
//! published, so it is always `<= recv_ts` (the message can't be received until
//! after it's enqueued) — the reported delay is a valid non-negative interval, not
//! a value that races the consumer and clamps to zero. For bounded/rendezvous
//! channels the stamp precedes the blocking send, so the delay includes
//! backpressure wait, not just queue residence. Both fields are internal — the
//! public API still sends and receives `T`.
//!
//! Because the wrapper rebuilds the inner channel, the channel expression passed
//! to `channel!(..., wrap = true)` must be constructed inline; raw endpoints must
//! not be cloned/retained before wrapping (such a clone would be orphaned).
//!
//! Returned types are [`Sender`]/[`Receiver`], re-exported as
//! `hotpath::wrap::crossbeam::{Sender, Receiver}`.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use crossbeam_channel::{
    Receiver as InnerReceiver, RecvError, RecvTimeoutError, SendError, SendTimeoutError,
    Sender as InnerSender, TryRecvError, TrySendError,
};

use crate::channels::{
    register_channel_wrap, send_channel_event, ChannelEvent, ChannelType, Instant,
    InstrumentChannelWrap, InstrumentChannelWrapLog,
};

/// Instrumented crossbeam [`crossbeam_channel::Sender`] wrapper.
///
/// Tracks every successful send and emits the exact channel length afterwards.
/// When the last clone is dropped, a `Closed` event is emitted.
pub struct Sender<T> {
    inner: InnerSender<(u64, Instant, T)>,
    id: u32,
    sender_count: Arc<AtomicUsize>,
    /// Monotonic message-id source, shared across all `Sender` clones so ids stay
    /// globally unique across producers.
    next_id: Arc<AtomicU64>,
    log_fn: Option<fn(&T) -> String>,
}

impl<T> Sender<T> {
    fn emit_sent(&self, msg_id: u64, sent_at: Instant, log: Option<String>) {
        send_channel_event(ChannelEvent::WrapMessageSent {
            id: self.id,
            msg_id,
            log,
            timestamp: sent_at,
            queue_len: self.inner.len(),
        });
    }

    pub fn send(&self, msg: T) -> Result<(), SendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        // Stamp before publishing and carry it in the envelope: a consumer can
        // receive and timestamp the message the instant `inner.send` enqueues it,
        // so sampling `now()` afterward races the receive and can read recv < send.
        // Captured here, send_ts <= recv_ts by construction.
        let sent_at = Instant::now();
        self.inner
            .send((msg_id, sent_at, msg))
            .map_err(|SendError((_, _, msg))| SendError(msg))?;
        self.emit_sent(msg_id, sent_at, log);
        Ok(())
    }

    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let sent_at = Instant::now();
        self.inner
            .try_send((msg_id, sent_at, msg))
            .map_err(|e| match e {
                TrySendError::Full((_, _, msg)) => TrySendError::Full(msg),
                TrySendError::Disconnected((_, _, msg)) => TrySendError::Disconnected(msg),
            })?;
        self.emit_sent(msg_id, sent_at, log);
        Ok(())
    }

    pub fn send_timeout(
        &self,
        msg: T,
        timeout: std::time::Duration,
    ) -> Result<(), SendTimeoutError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let sent_at = Instant::now();
        self.inner
            .send_timeout((msg_id, sent_at, msg), timeout)
            .map_err(|e| match e {
                SendTimeoutError::Timeout((_, _, msg)) => SendTimeoutError::Timeout(msg),
                SendTimeoutError::Disconnected((_, _, msg)) => SendTimeoutError::Disconnected(msg),
            })?;
        self.emit_sent(msg_id, sent_at, log);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.inner.is_full()
    }

    pub fn capacity(&self) -> Option<usize> {
        self.inner.capacity()
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.sender_count.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: self.inner.clone(),
            id: self.id,
            sender_count: Arc::clone(&self.sender_count),
            next_id: Arc::clone(&self.next_id),
            log_fn: self.log_fn,
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if self.sender_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            send_channel_event(ChannelEvent::Closed { id: self.id });
        }
    }
}

/// Instrumented crossbeam [`crossbeam_channel::Receiver`] wrapper.
///
/// Tracks every successful receive and emits the exact channel length afterwards.
/// When the last clone is dropped, a `Closed` event is emitted, since dropping all
/// receivers disconnects the channel.
pub struct Receiver<T> {
    inner: InnerReceiver<(u64, Instant, T)>,
    id: u32,
    receiver_count: Arc<AtomicUsize>,
}

impl<T> Receiver<T> {
    /// Inner receiver for use with `crossbeam_channel::Select`. Register it, wait
    /// with `Select::ready`/`ready_timeout`, then receive via this wrapper's
    /// `try_recv`/`recv` so instrumentation still fires.
    pub fn select_handle(&self) -> &InnerReceiver<(u64, Instant, T)> {
        &self.inner
    }

    fn on_received(&self, msg_id: u64) {
        send_channel_event(ChannelEvent::WrapMessageReceived {
            id: self.id,
            msg_id,
            timestamp: Instant::now(),
            queue_len: self.inner.len(),
        });
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        // `send_ts` rides in the envelope; Phase 2 computes the delay here
        // (`now() - send_ts`) and records it straight into the histogram.
        let (msg_id, _send_ts, msg) = self.inner.recv()?;
        self.on_received(msg_id);
        Ok(msg)
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let (msg_id, _send_ts, msg) = self.inner.try_recv()?;
        self.on_received(msg_id);
        Ok(msg)
    }

    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Result<T, RecvTimeoutError> {
        let (msg_id, _send_ts, msg) = self.inner.recv_timeout(timeout)?;
        self.on_received(msg_id);
        Ok(msg)
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter { rx: self }
    }

    pub fn try_iter(&self) -> TryIter<'_, T> {
        TryIter { rx: self }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.inner.is_full()
    }

    pub fn capacity(&self) -> Option<usize> {
        self.inner.capacity()
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.receiver_count.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: self.inner.clone(),
            id: self.id,
            receiver_count: Arc::clone(&self.receiver_count),
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if self.receiver_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            send_channel_event(ChannelEvent::Closed { id: self.id });
        }
    }
}

/// Blocking iterator over a [`Receiver`], mirroring `crossbeam_channel::Iter`.
pub struct Iter<'a, T> {
    rx: &'a Receiver<T>,
}

impl<T> Iterator for Iter<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.rx.recv().ok()
    }
}

/// Non-blocking iterator over a [`Receiver`], mirroring `crossbeam_channel::TryIter`.
pub struct TryIter<'a, T> {
    rx: &'a Receiver<T>,
}

impl<T> Iterator for TryIter<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.rx.try_recv().ok()
    }
}

/// Owning blocking iterator, mirroring `crossbeam_channel::IntoIter`.
pub struct IntoIter<T> {
    rx: Receiver<T>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.rx.recv().ok()
    }
}

impl<T> IntoIterator for Receiver<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> IntoIter<T> {
        IntoIter { rx: self }
    }
}

impl<'a, T> IntoIterator for &'a Receiver<T> {
    type Item = T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

fn channel_type<T>(tx: &InnerSender<T>) -> ChannelType {
    match tx.capacity() {
        Some(capacity) => ChannelType::Bounded(capacity),
        None => ChannelType::Unbounded,
    }
}

fn build<T>(
    inner: (InnerSender<T>, InnerReceiver<T>),
    source: &'static str,
    label: Option<String>,
    log_fn: Option<fn(&T) -> String>,
) -> (Sender<T>, Receiver<T>) {
    let (orig_tx, _orig_rx) = inner;
    let ch_type = channel_type(&orig_tx);
    let id = register_channel_wrap::<T>(source, label, ch_type);

    // Rebuild the inner channel to carry `(msg_id, send_ts, T)` so each message's
    // identity and send time travel with it. The caller's original channel is
    // discarded — wrap mode is inline-construction only (see module docs); only its
    // kind/capacity is copied.
    let (tx, rx) = match ch_type {
        ChannelType::Bounded(cap) => crossbeam_channel::bounded::<(u64, Instant, T)>(cap),
        ChannelType::Unbounded => crossbeam_channel::unbounded::<(u64, Instant, T)>(),
        ChannelType::Oneshot => crossbeam_channel::bounded::<(u64, Instant, T)>(1),
    };

    let sender = Sender {
        inner: tx,
        id,
        sender_count: Arc::new(AtomicUsize::new(1)),
        next_id: Arc::new(AtomicU64::new(0)),
        log_fn,
    };
    let receiver = Receiver {
        inner: rx,
        id,
        receiver_count: Arc::new(AtomicUsize::new(1)),
    };
    (sender, receiver)
}

impl<T: Send + 'static> InstrumentChannelWrap for (InnerSender<T>, InnerReceiver<T>) {
    type Output = (Sender<T>, Receiver<T>);
    fn instrument_wrap(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        build(self, source, label, None)
    }
}

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelWrapLog
    for (InnerSender<T>, InnerReceiver<T>)
{
    type Output = (Sender<T>, Receiver<T>);
    fn instrument_wrap_log(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        let log_fn: fn(&T) -> String = |m| crate::output::format_debug_truncated(m);
        build(self, source, label, Some(log_fn))
    }
}
