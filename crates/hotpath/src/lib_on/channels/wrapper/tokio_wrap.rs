//! Endpoint-wrapping Tokio `mpsc` channel instrumentation for the `channel!` macro.
//!
//! Wraps the `Sender`/`Receiver` endpoints directly, unlike the forwarder-proxy in
//! [`crate::channels::wrapper::tokio`], which spawns a background task that relays every
//! message through a second channel. That forwarder costs a scheduler round-trip per
//! message (the message is not visible to `recv` until the relay task is polled); wrap
//! mode removes the task and the second channel, so send/recv hit the real channel and
//! the only added cost is a non-blocking event emit.
//!
//! Tokio `mpsc` exposes no cheap exact `len()` on the sender side, so `queue_len` is read
//! from a self-maintained `AtomicUsize`: incremented before each publish (rolled back if
//! the send fails) and decremented after each receive. Counting before the publish keeps
//! the counter non-negative - the channel's send->recv edge orders a producer's `+1` ahead
//! of the consumer's matching `-1`. A bounded async `send` that is cancelled while parked
//! on a full channel leaves its `+1` applied (the depth over-counts by the number of
//! cancelled sends); successful and failed sends are exact.
//!
//! The inner channel carries `(msg_id, send_ts, T)`. Monotonic `msg_id` pairs a send with
//! its matching receive under multiple producers; `send_ts` is stamped before publishing,
//! so `send_ts <= recv_ts` always holds and the reported delay is non-negative. For
//! bounded channels it precedes the (awaited) send, so the delay includes backpressure
//! wait. Both fields are internal - the public API still uses `T`.
//!
//! The wrapper rebuilds the inner channel, so the `channel!` expression must be
//! constructed inline; endpoints cloned before wrapping are orphaned. Bounded capacity is
//! recovered from `Sender::max_capacity()`, so no `capacity = N` argument is needed.
//!
//! Tokio `Receiver` is single-consumer (not `Clone`), so there is exactly one receiver and
//! it emits `Closed` unconditionally on drop.
//!
//! Returns [`Sender`]/[`Receiver`]/[`UnboundedSender`]/[`UnboundedReceiver`], re-exported as
//! `hotpath::wrap::tokio::sync::mpsc::*`.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::{SendError, TryRecvError, TrySendError};

use crate::channels::{
    register_channel_wrap, send_channel_event, ChannelEvent, ChannelType, Instant,
    InstrumentChannelWrap, InstrumentChannelWrapLog,
};

type Payload<T> = (u64, Instant, T);

/// `send_ts` is stamped before the (possibly awaited) send, so it is always `<= now`.
#[inline]
fn delay_nanos(send_ts: Instant, now: Instant) -> u64 {
    now.duration_since(send_ts).as_nanos() as u64
}

#[inline]
fn clamp_to_capacity(queue_len: usize, capacity: Option<usize>) -> usize {
    match capacity {
        Some(cap) => queue_len.min(cap),
        None => queue_len,
    }
}

fn emit_sent(id: u32, msg_id: u64, sent_at: Instant, log: Option<String>, queue_len: usize) {
    send_channel_event(ChannelEvent::WrapMessageSent {
        id,
        msg_id,
        log,
        timestamp: sent_at,
        queue_len,
    });
}

fn emit_received(id: u32, msg_id: u64, now: Instant, queue_len: usize, delay_nanos: u64) {
    send_channel_event(ChannelEvent::WrapMessageReceived {
        id,
        msg_id,
        timestamp: now,
        queue_len,
        delay_nanos,
    });
}

/// Instrumented bounded [`tokio::sync::mpsc::Sender`] wrapper.
pub struct Sender<T> {
    inner: mpsc::Sender<Payload<T>>,
    id: u32,
    capacity: usize,
    sender_count: Arc<AtomicUsize>,
    next_id: Arc<AtomicU64>,
    depth: Arc<AtomicUsize>,
    log_fn: Option<fn(&T) -> String>,
}

impl<T> Sender<T> {
    pub async fn send(&self, msg: T) -> Result<(), SendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let sent_at = Instant::now();
        let queue_len = (self.depth.fetch_add(1, Ordering::Relaxed) + 1).min(self.capacity);
        match self.inner.send((msg_id, sent_at, msg)).await {
            Ok(()) => {
                emit_sent(self.id, msg_id, sent_at, log, queue_len);
                Ok(())
            }
            Err(SendError((_, _, msg))) => {
                self.depth.fetch_sub(1, Ordering::Relaxed);
                Err(SendError(msg))
            }
        }
    }

    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let sent_at = Instant::now();
        let queue_len = (self.depth.fetch_add(1, Ordering::Relaxed) + 1).min(self.capacity);
        match self.inner.try_send((msg_id, sent_at, msg)) {
            Ok(()) => {
                emit_sent(self.id, msg_id, sent_at, log, queue_len);
                Ok(())
            }
            Err(e) => {
                self.depth.fetch_sub(1, Ordering::Relaxed);
                Err(match e {
                    TrySendError::Full((_, _, msg)) => TrySendError::Full(msg),
                    TrySendError::Closed((_, _, msg)) => TrySendError::Closed(msg),
                })
            }
        }
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    pub fn max_capacity(&self) -> usize {
        self.inner.max_capacity()
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.sender_count.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: self.inner.clone(),
            id: self.id,
            capacity: self.capacity,
            sender_count: Arc::clone(&self.sender_count),
            next_id: Arc::clone(&self.next_id),
            depth: Arc::clone(&self.depth),
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

/// Instrumented bounded [`tokio::sync::mpsc::Receiver`] wrapper (single consumer).
pub struct Receiver<T> {
    inner: mpsc::Receiver<Payload<T>>,
    id: u32,
    capacity: Option<usize>,
    depth: Arc<AtomicUsize>,
}

impl<T> Receiver<T> {
    fn on_received(&self, msg_id: u64, now: Instant, delay_nanos: u64) {
        let queue_len = clamp_to_capacity(
            self.depth.fetch_sub(1, Ordering::Relaxed) - 1,
            self.capacity,
        );
        emit_received(self.id, msg_id, now, queue_len, delay_nanos);
    }

    pub async fn recv(&mut self) -> Option<T> {
        let (msg_id, send_ts, msg) = self.inner.recv().await?;
        let now = Instant::now();
        self.on_received(msg_id, now, delay_nanos(send_ts, now));
        Some(msg)
    }

    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let (msg_id, send_ts, msg) = self.inner.try_recv()?;
        let now = Instant::now();
        self.on_received(msg_id, now, delay_nanos(send_ts, now));
        Ok(msg)
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        send_channel_event(ChannelEvent::Closed { id: self.id });
    }
}

/// Instrumented [`tokio::sync::mpsc::UnboundedSender`] wrapper.
pub struct UnboundedSender<T> {
    inner: mpsc::UnboundedSender<Payload<T>>,
    id: u32,
    sender_count: Arc<AtomicUsize>,
    next_id: Arc<AtomicU64>,
    depth: Arc<AtomicUsize>,
    log_fn: Option<fn(&T) -> String>,
}

impl<T> UnboundedSender<T> {
    pub fn send(&self, msg: T) -> Result<(), SendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let sent_at = Instant::now();
        let queue_len = self.depth.fetch_add(1, Ordering::Relaxed) + 1;
        match self.inner.send((msg_id, sent_at, msg)) {
            Ok(()) => {
                emit_sent(self.id, msg_id, sent_at, log, queue_len);
                Ok(())
            }
            Err(SendError((_, _, msg))) => {
                self.depth.fetch_sub(1, Ordering::Relaxed);
                Err(SendError(msg))
            }
        }
    }
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        self.sender_count.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: self.inner.clone(),
            id: self.id,
            sender_count: Arc::clone(&self.sender_count),
            next_id: Arc::clone(&self.next_id),
            depth: Arc::clone(&self.depth),
            log_fn: self.log_fn,
        }
    }
}

impl<T> Drop for UnboundedSender<T> {
    fn drop(&mut self) {
        if self.sender_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            send_channel_event(ChannelEvent::Closed { id: self.id });
        }
    }
}

/// Instrumented [`tokio::sync::mpsc::UnboundedReceiver`] wrapper (single consumer).
pub struct UnboundedReceiver<T> {
    inner: mpsc::UnboundedReceiver<Payload<T>>,
    id: u32,
    depth: Arc<AtomicUsize>,
}

impl<T> UnboundedReceiver<T> {
    fn on_received(&self, msg_id: u64, now: Instant, delay_nanos: u64) {
        let queue_len = self.depth.fetch_sub(1, Ordering::Relaxed) - 1;
        emit_received(self.id, msg_id, now, queue_len, delay_nanos);
    }

    pub async fn recv(&mut self) -> Option<T> {
        let (msg_id, send_ts, msg) = self.inner.recv().await?;
        let now = Instant::now();
        self.on_received(msg_id, now, delay_nanos(send_ts, now));
        Some(msg)
    }

    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let (msg_id, send_ts, msg) = self.inner.try_recv()?;
        let now = Instant::now();
        self.on_received(msg_id, now, delay_nanos(send_ts, now));
        Ok(msg)
    }
}

impl<T> Drop for UnboundedReceiver<T> {
    fn drop(&mut self) {
        send_channel_event(ChannelEvent::Closed { id: self.id });
    }
}

fn build_bounded<T>(
    inner: (mpsc::Sender<T>, mpsc::Receiver<T>),
    source: &'static str,
    label: Option<String>,
    log_fn: Option<fn(&T) -> String>,
) -> (Sender<T>, Receiver<T>) {
    let capacity = inner.0.max_capacity();
    let id = register_channel_wrap::<T>(source, label, ChannelType::Bounded(capacity));
    // Rebuild to carry `(msg_id, send_ts, T)`; the caller's channel is discarded
    // (wrap mode is inline-only), only its capacity is copied.
    let (tx, rx) = mpsc::channel::<Payload<T>>(capacity);
    let depth = Arc::new(AtomicUsize::new(0));
    let sender = Sender {
        inner: tx,
        id,
        capacity,
        sender_count: Arc::new(AtomicUsize::new(1)),
        next_id: Arc::new(AtomicU64::new(0)),
        depth: Arc::clone(&depth),
        log_fn,
    };
    let receiver = Receiver {
        inner: rx,
        id,
        capacity: Some(capacity),
        depth,
    };
    (sender, receiver)
}

fn build_unbounded<T>(
    source: &'static str,
    label: Option<String>,
    log_fn: Option<fn(&T) -> String>,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let id = register_channel_wrap::<T>(source, label, ChannelType::Unbounded);
    let (tx, rx) = mpsc::unbounded_channel::<Payload<T>>();
    let depth = Arc::new(AtomicUsize::new(0));
    let sender = UnboundedSender {
        inner: tx,
        id,
        sender_count: Arc::new(AtomicUsize::new(1)),
        next_id: Arc::new(AtomicU64::new(0)),
        depth: Arc::clone(&depth),
        log_fn,
    };
    let receiver = UnboundedReceiver {
        inner: rx,
        id,
        depth,
    };
    (sender, receiver)
}

impl<T: Send + 'static> InstrumentChannelWrap for (mpsc::Sender<T>, mpsc::Receiver<T>) {
    type Output = (Sender<T>, Receiver<T>);
    fn instrument_wrap(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        build_bounded(self, source, label, None)
    }
}

impl<T: Send + 'static> InstrumentChannelWrap
    for (mpsc::UnboundedSender<T>, mpsc::UnboundedReceiver<T>)
{
    type Output = (UnboundedSender<T>, UnboundedReceiver<T>);
    fn instrument_wrap(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        build_unbounded(source, label, None)
    }
}

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelWrapLog
    for (mpsc::Sender<T>, mpsc::Receiver<T>)
{
    type Output = (Sender<T>, Receiver<T>);
    fn instrument_wrap_log(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        let log_fn: fn(&T) -> String = |m| crate::output::format_debug_truncated(m);
        build_bounded(self, source, label, Some(log_fn))
    }
}

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelWrapLog
    for (mpsc::UnboundedSender<T>, mpsc::UnboundedReceiver<T>)
{
    type Output = (UnboundedSender<T>, UnboundedReceiver<T>);
    fn instrument_wrap_log(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        let log_fn: fn(&T) -> String = |m| crate::output::format_debug_truncated(m);
        build_unbounded(source, label, Some(log_fn))
    }
}
