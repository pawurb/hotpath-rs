//! Endpoint-wrapping crossbeam channel instrumentation for the `channel!` macro.
//!
//! Wraps the `Sender`/`Receiver` endpoints directly (unlike the forwarder-proxy in
//! [`crate::channels::wrapper::crossbeam`]): no extra thread or proxy channel, so
//! send/recv hit the real channel. `queue_len` is a snapshot taken right after each
//! op - exact single-threaded, may skew under concurrent endpoints.
//!
//! The inner channel carries `(msg_id, send_ts, T)`. Monotonic `msg_id` pairs a send
//! with its matching receive under multiple producers/consumers. `send_ts` is stamped
//! before publishing, so `send_ts <= recv_ts` always holds and the reported delay is
//! non-negative (no race with the consumer that clamps to zero); for bounded/rendezvous
//! channels it precedes the blocking send, so the delay includes backpressure wait.
//! Both fields are internal - the public API still uses `T`.
//!
//! The wrapper rebuilds the inner channel, so the `channel!` expression must be
//! constructed inline; endpoints cloned before wrapping are orphaned.
//!
//! Returns [`Sender`]/[`Receiver`], re-exported as
//! `hotpath::wrap::crossbeam_channel::{Sender, Receiver}`.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
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
    inner: InnerSender<(u64, Option<Instant>, T)>,
    id: u32,
    sender_count: Arc<AtomicUsize>,
    /// Monotonic message-id source, shared across all `Sender` clones so ids stay
    /// globally unique across producers.
    next_id: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
    log_fn: Option<fn(&T) -> String>,
}

impl<T> Sender<T> {
    fn emit_sent(&self, msg_id: u64, sent_at: Option<Instant>, log: Option<String>) {
        send_channel_event(ChannelEvent::WrapMessageSent {
            id: self.id,
            msg_id,
            log,
            timestamp: crate::channels::anchor_first_msg(msg_id, sent_at),
            queue_len: self.inner.len(),
        });
    }

    pub fn send(&self, msg: T) -> Result<(), SendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        // Stamp before publishing: a consumer could receive and timestamp the message
        // the instant `inner.send` enqueues it, so stamping after would race the
        // receive and read recv < send. Here send_ts <= recv_ts by construction.
        let sent_at = sample_stamp(msg_id);
        self.inner
            .send((msg_id, sent_at, msg))
            .map_err(|SendError((_, _, msg))| SendError(msg))?;
        self.emit_sent(msg_id, sent_at, log);
        Ok(())
    }

    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let sent_at = sample_stamp(msg_id);
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
        let sent_at = sample_stamp(msg_id);
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
            closed: Arc::clone(&self.closed),
            log_fn: self.log_fn,
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if self.sender_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            crate::channels::mark_closed(&self.closed, self.id, self.inner.len());
        }
    }
}

/// Instrumented crossbeam [`crossbeam_channel::Receiver`] wrapper.
///
/// Tracks every successful receive and emits the exact channel length afterwards.
/// When the last clone is dropped, a `Closed` event is emitted, since dropping all
/// receivers disconnects the channel.
pub struct Receiver<T> {
    inner: InnerReceiver<(u64, Option<Instant>, T)>,
    id: u32,
    receiver_count: Arc<AtomicUsize>,
    closed: Arc<AtomicBool>,
}

impl<T> Receiver<T> {
    /// Inner receiver for use with `crossbeam_channel::Select`. Register it, wait
    /// with `Select::ready`/`ready_timeout`, then receive via this wrapper's
    /// `try_recv`/`recv` so instrumentation still fires.
    pub fn select_handle(&self) -> &InnerReceiver<(u64, Option<Instant>, T)> {
        &self.inner
    }

    fn on_received(&self, msg_id: u64, now: Option<Instant>, delay_nanos: Option<u64>) {
        send_channel_event(ChannelEvent::WrapMessageReceived {
            id: self.id,
            msg_id,
            timestamp: now,
            queue_len: self.inner.len(),
            delay_nanos,
        });
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        // `send_ts` rides in the envelope; the delay (`now - send_ts`) is the exact
        // send->receive latency, recorded straight into the processing-time histogram.
        let (msg_id, send_ts, msg) = self.inner.recv()?;
        let (now, delay) = recv_stamp(send_ts);
        self.on_received(msg_id, now, delay);
        Ok(msg)
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let (msg_id, send_ts, msg) = self.inner.try_recv()?;
        let (now, delay) = recv_stamp(send_ts);
        self.on_received(msg_id, now, delay);
        Ok(msg)
    }

    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Result<T, RecvTimeoutError> {
        let (msg_id, send_ts, msg) = self.inner.recv_timeout(timeout)?;
        let (now, delay) = recv_stamp(send_ts);
        self.on_received(msg_id, now, delay);
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
            closed: Arc::clone(&self.closed),
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if self.receiver_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            crate::channels::mark_closed(&self.closed, self.id, self.inner.len());
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

/// `send_ts` is stamped before the (possibly blocking) `send`, so it is always `<= now`.
#[inline]
fn delay_nanos(send_ts: Instant, now: Instant) -> u64 {
    now.duration_since(send_ts).as_nanos() as u64
}

/// Send-side sampling decision keyed on `msg_id % k`; `None` skips the clock
/// read and travels in the payload so the receiver skips its read too.
#[inline]
fn sample_stamp(msg_id: u64) -> Option<Instant> {
    crate::lib_on::sampling::channels_should_time(msg_id).then(Instant::now)
}

/// A `Some` payload stamp means the message is sampled: stamp `now`, compute the delay.
#[inline]
fn recv_stamp(send_ts: Option<Instant>) -> (Option<Instant>, Option<u64>) {
    match send_ts {
        Some(ts) => {
            let now = Instant::now();
            (Some(now), Some(delay_nanos(ts, now)))
        }
        None => (None, None),
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
    iter: bool,
) -> (Sender<T>, Receiver<T>) {
    let (orig_tx, _orig_rx) = inner;
    let ch_type = channel_type(&orig_tx);
    let id = register_channel_wrap::<T>(source, label, ch_type, iter);
    let closed = Arc::new(AtomicBool::new(false));
    // Aggregated instances share one msg-id sequence so ids stay unique
    // within the entry; iter-mode instances keep a local counter.
    let next_id = if iter {
        Arc::new(AtomicU64::new(0))
    } else {
        crate::channels::entry_msg_counter(id)
    };

    // Rebuild the inner channel to carry `(msg_id, send_ts, T)`. The caller's original
    // channel is discarded (wrap mode is inline-only, see module docs); only its
    // kind/capacity is copied.
    let (tx, rx) = match ch_type {
        ChannelType::Bounded(cap) => crossbeam_channel::bounded::<(u64, Option<Instant>, T)>(cap),
        ChannelType::Unbounded => crossbeam_channel::unbounded::<(u64, Option<Instant>, T)>(),
        ChannelType::Oneshot => crossbeam_channel::bounded::<(u64, Option<Instant>, T)>(1),
        // `channel_type` above only returns Bounded or Unbounded.
        ChannelType::Pending => unreachable!(),
    };

    let sender = Sender {
        inner: tx,
        id,
        sender_count: Arc::new(AtomicUsize::new(1)),
        next_id,
        closed: Arc::clone(&closed),
        log_fn,
    };
    let receiver = Receiver {
        inner: rx,
        id,
        receiver_count: Arc::new(AtomicUsize::new(1)),
        closed,
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
        iter: bool,
    ) -> Self::Output {
        build(self, source, label, None, iter)
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
        iter: bool,
    ) -> Self::Output {
        let log_fn: fn(&T) -> String = |m| crate::output_on::format_debug_truncated(m);
        build(self, source, label, Some(log_fn), iter)
    }
}
