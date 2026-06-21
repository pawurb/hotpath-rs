//! Endpoint-wrapping `std::sync::mpsc` channel instrumentation (`channel!(..., wrap = true)`).
//!
//! Wraps the `Sender`/`SyncSender`/`Receiver` endpoints directly (unlike the
//! forwarder-proxy in [`crate::channels::wrapper::std`]): no extra thread or proxy
//! channel, so send/recv hit the real channel.
//!
//! `std::sync::mpsc` exposes no public `len()`, so `queue_len` is read from a
//! self-maintained `AtomicUsize`: incremented before each publish (rolled back if the
//! send fails) and decremented after each receive. Counting before the publish is what
//! keeps the counter non-negative - the channel's send->recv edge orders a producer's
//! `+1` ahead of the consumer's matching `-1`, so a fast consumer can never decrement a
//! slot that has not yet been counted. The snapshot stays exact even under concurrent
//! producers - more accurate than a library-provided racy length snapshot.
//!
//! The inner channel carries `(msg_id, send_ts, T)`. Monotonic `msg_id` pairs a send
//! with its matching receive under multiple producers. `send_ts` is stamped before
//! publishing, so `send_ts <= recv_ts` always holds and the reported delay is
//! non-negative; for bounded (`SyncSender`) channels it precedes the blocking send, so
//! the delay includes backpressure wait. Both fields are internal - the public API
//! still uses `T`.
//!
//! The wrapper rebuilds the inner channel, so the `channel!` expression must be
//! constructed inline; endpoints cloned before wrapping are orphaned.
//!
//! For bounded channels (`SyncSender`) the rebuilt channel uses the `capacity = N` macro
//! argument, not the discarded `sync_channel(M)` you passed (std exposes no capacity
//! accessor to read `M`). The two must be equal: a mismatch changes the channel's
//! backpressure only in profiled builds (the `hotpath-meta`-off `channel!` keeps your
//! original `M`), which can manifest as a deadlock that disappears when profiling is
//! disabled.
//!
//! `std::sync::mpsc::Receiver` is single-consumer (not `Clone`), so there is exactly one
//! receiver and it emits `Closed` unconditionally on drop.
//!
//! Returns [`Sender`]/[`SyncSender`]/[`Receiver`], re-exported as
//! `hotpath_meta::wrap::std::sync::mpsc::{Sender, SyncSender, Receiver}`.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{
    self, Receiver as InnerReceiver, RecvError, RecvTimeoutError, SendError, Sender as InnerSender,
    SyncSender as InnerSyncSender, TryRecvError, TrySendError,
};
use std::sync::Arc;

use crate::channels::{
    register_channel_wrap, send_channel_event, ChannelEvent, ChannelType, Instant,
    InstrumentChannelWrap, InstrumentChannelWrapLog,
};

type Payload<T> = (u64, Instant, T);

/// `send_ts` is stamped before the (possibly blocking) send, so it is always `<= now`.
#[inline]
fn delay_nanos(send_ts: Instant, now: Instant) -> u64 {
    now.duration_since(send_ts).as_nanos() as u64
}

/// The self-tracked depth can transiently read `capacity + 1`: the increment runs
/// after `send` returns and the decrement after `recv` returns, so a producer unblocked
/// by a consumer may apply its `+1` before that consumer applies its `-1`. Clamp to the
/// declared capacity (bounded only) so the reported depth never exceeds the real bound.
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

/// Instrumented [`std::sync::mpsc::Sender`] (unbounded) wrapper.
///
/// Tracks every successful send and emits the self-tracked queue depth afterwards.
/// When the last clone is dropped, a `Closed` event is emitted.
pub struct Sender<T> {
    inner: InnerSender<Payload<T>>,
    id: u32,
    sender_count: Arc<AtomicUsize>,
    /// Monotonic message-id source, shared across all `Sender` clones so ids stay
    /// globally unique across producers.
    next_id: Arc<AtomicU64>,
    /// Self-tracked queue depth, shared with the receiver (`std` exposes no `len()`).
    depth: Arc<AtomicUsize>,
    log_fn: Option<fn(&T) -> String>,
}

impl<T> Sender<T> {
    pub fn send(&self, msg: T) -> Result<(), SendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        // Stamp before publishing: a consumer could receive and timestamp the message
        // the instant `inner.send` enqueues it, so stamping after would race the
        // receive and read recv < send. Here send_ts <= recv_ts by construction.
        let sent_at = Instant::now();
        // Count before publishing so the receiver never decrements a slot that has not
        // yet been counted; roll back if the publish fails.
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

impl<T> Clone for Sender<T> {
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

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if self.sender_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            send_channel_event(ChannelEvent::Closed { id: self.id });
        }
    }
}

/// Instrumented [`std::sync::mpsc::SyncSender`] (bounded) wrapper.
///
/// Tracks every successful send and emits the self-tracked queue depth afterwards.
/// When the last clone is dropped, a `Closed` event is emitted.
pub struct SyncSender<T> {
    inner: InnerSyncSender<Payload<T>>,
    id: u32,
    capacity: usize,
    sender_count: Arc<AtomicUsize>,
    next_id: Arc<AtomicU64>,
    depth: Arc<AtomicUsize>,
    log_fn: Option<fn(&T) -> String>,
}

impl<T> SyncSender<T> {
    pub fn send(&self, msg: T) -> Result<(), SendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        // Stamped before the blocking send, so the recorded delay includes backpressure
        // wait while the bounded channel is full.
        let sent_at = Instant::now();
        // Count before publishing so the receiver never decrements a slot that has not
        // yet been counted; roll back if the publish fails.
        let queue_len = (self.depth.fetch_add(1, Ordering::Relaxed) + 1).min(self.capacity);
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
                    TrySendError::Disconnected((_, _, msg)) => TrySendError::Disconnected(msg),
                })
            }
        }
    }
}

impl<T> Clone for SyncSender<T> {
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

impl<T> Drop for SyncSender<T> {
    fn drop(&mut self) {
        if self.sender_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            send_channel_event(ChannelEvent::Closed { id: self.id });
        }
    }
}

/// Instrumented [`std::sync::mpsc::Receiver`] (single consumer) wrapper.
///
/// Tracks every successful receive and emits the self-tracked queue depth afterwards.
/// `std` receivers are not `Clone`, so this is the sole consumer; it emits `Closed`
/// unconditionally on drop.
pub struct Receiver<T> {
    inner: InnerReceiver<Payload<T>>,
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
        send_channel_event(ChannelEvent::WrapMessageReceived {
            id: self.id,
            msg_id,
            timestamp: now,
            queue_len,
            delay_nanos,
        });
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        // `send_ts` rides in the envelope; the delay (`now - send_ts`) is the exact
        // send->receive latency, recorded straight into the processing-time histogram.
        let (msg_id, send_ts, msg) = self.inner.recv()?;
        let now = Instant::now();
        self.on_received(msg_id, now, delay_nanos(send_ts, now));
        Ok(msg)
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let (msg_id, send_ts, msg) = self.inner.try_recv()?;
        let now = Instant::now();
        self.on_received(msg_id, now, delay_nanos(send_ts, now));
        Ok(msg)
    }

    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Result<T, RecvTimeoutError> {
        let (msg_id, send_ts, msg) = self.inner.recv_timeout(timeout)?;
        let now = Instant::now();
        self.on_received(msg_id, now, delay_nanos(send_ts, now));
        Ok(msg)
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter { rx: self }
    }

    pub fn try_iter(&self) -> TryIter<'_, T> {
        TryIter { rx: self }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        send_channel_event(ChannelEvent::Closed { id: self.id });
    }
}

/// Blocking iterator over a [`Receiver`], mirroring `std::sync::mpsc::Iter`.
pub struct Iter<'a, T> {
    rx: &'a Receiver<T>,
}

impl<T> Iterator for Iter<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.rx.recv().ok()
    }
}

/// Non-blocking iterator over a [`Receiver`], mirroring `std::sync::mpsc::TryIter`.
pub struct TryIter<'a, T> {
    rx: &'a Receiver<T>,
}

impl<T> Iterator for TryIter<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.rx.try_recv().ok()
    }
}

/// Owning blocking iterator, mirroring `std::sync::mpsc::IntoIter`.
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

fn build_unbounded<T>(
    source: &'static str,
    label: Option<String>,
    log_fn: Option<fn(&T) -> String>,
) -> (Sender<T>, Receiver<T>) {
    let id = register_channel_wrap::<T>(source, label, ChannelType::Unbounded);
    let (tx, rx) = mpsc::channel::<Payload<T>>();
    let depth = Arc::new(AtomicUsize::new(0));
    let sender = Sender {
        inner: tx,
        id,
        sender_count: Arc::new(AtomicUsize::new(1)),
        next_id: Arc::new(AtomicU64::new(0)),
        depth: Arc::clone(&depth),
        log_fn,
    };
    let receiver = Receiver {
        inner: rx,
        id,
        capacity: None,
        depth,
    };
    (sender, receiver)
}

fn build_bounded<T>(
    source: &'static str,
    label: Option<String>,
    capacity: usize,
    log_fn: Option<fn(&T) -> String>,
) -> (SyncSender<T>, Receiver<T>) {
    let id = register_channel_wrap::<T>(source, label, ChannelType::Bounded(capacity));
    let (tx, rx) = mpsc::sync_channel::<Payload<T>>(capacity);
    let depth = Arc::new(AtomicUsize::new(0));
    let sender = SyncSender {
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

fn require_capacity(capacity: Option<usize>) -> usize {
    capacity.unwrap_or_else(|| {
        panic!("bounded std::sync::mpsc wrap requires `capacity = N` (std exposes no capacity accessor); it must match the sync_channel(N) argument, e.g. channel!(mpsc::sync_channel::<T>(100), wrap = true, capacity = 100)")
    })
}

impl<T: Send + 'static> InstrumentChannelWrap for (InnerSender<T>, InnerReceiver<T>) {
    type Output = (Sender<T>, Receiver<T>);
    fn instrument_wrap(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
    ) -> Self::Output {
        build_unbounded(source, label, None)
    }
}

impl<T: Send + 'static> InstrumentChannelWrap for (InnerSyncSender<T>, InnerReceiver<T>) {
    type Output = (SyncSender<T>, Receiver<T>);
    fn instrument_wrap(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
    ) -> Self::Output {
        build_bounded(source, label, require_capacity(capacity), None)
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
        build_unbounded(source, label, Some(log_fn))
    }
}

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelWrapLog
    for (InnerSyncSender<T>, InnerReceiver<T>)
{
    type Output = (SyncSender<T>, Receiver<T>);
    fn instrument_wrap_log(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
    ) -> Self::Output {
        let log_fn: fn(&T) -> String = |m| crate::output::format_debug_truncated(m);
        build_bounded(source, label, require_capacity(capacity), Some(log_fn))
    }
}
