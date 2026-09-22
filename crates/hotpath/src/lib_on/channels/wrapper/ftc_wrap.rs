//! Endpoint-wrapping `futures_channel::mpsc` instrumentation for the `channel!` macro.
//!
//! Wraps the `Sender`/`Receiver` endpoints directly: sends and receives hit the real
//! channel and the only added cost is a non-blocking event emit.
//!
//! futures `mpsc` exposes no `len()` on the bounded endpoints, so `queue_len` is read
//! from a self-maintained `AtomicUsize`: incremented before each push (rolled back if the
//! push fails) and decremented after each receive. Counting before the push keeps the
//! counter non-negative - the channel's send->recv edge orders a producer's `+1` ahead of
//! the consumer's matching `-1`. Pushes never park (`poll_ready` does the waiting), so no
//! cancellation guard is needed. Exact for a single producer; with concurrent cloned
//! senders another sender's pre-push `+1` can land in a snapshot, so depth and high-water
//! marks may transiently read one higher per in-flight send - same tradeoff as the tokio
//! wrapper.
//!
//! The inner channel carries `(msg_id, send_ts, T)`. Monotonic `msg_id` pairs a send with
//! its matching receive under multiple producers; `send_ts` is stamped before publishing,
//! so `send_ts <= recv_ts` always holds and the reported delay is non-negative. Both fields
//! are internal - the public API still uses `T`.
//!
//! `mpsc::TrySendError` has no public constructor, so `try_send` / `unbounded_send` return
//! this module's [`TrySendError`], which keeps the inner error whole and exposes the same
//! methods (`is_full`, `is_disconnected`, `into_inner`, `into_send_error`). `Sink` errors
//! are the plain `mpsc::SendError` and pass through unchanged.
//!
//! The wrapper rebuilds the inner channel, so the `channel!` expression must be
//! constructed inline; endpoints cloned before wrapping are orphaned. Bounded channels
//! expose no capacity accessor, so `capacity = N` is required and must match the
//! `channel(N)` argument.
//!
//! Receivers are single-consumer (not `Clone`) and emit `Closed` unconditionally on drop.
//!
//! Returns [`Sender`]/[`Receiver`]/[`UnboundedSender`]/[`UnboundedReceiver`], re-exported
//! as `hotpath::wrap::futures_channel::mpsc::*`.

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_channel::mpsc;
use futures_channel::mpsc::{RecvError, SendError, TryRecvError};
use futures_core::stream::{FusedStream, Stream};
use futures_sink::Sink;

use crate::channels::{
    register_channel, sample_stamp, send_channel_event, ChannelEvent, ChannelType, Instant,
    InstrumentChannelWrap, InstrumentChannelWrapLog,
};

type Payload<T> = (u64, Option<Instant>, T);

/// `send_ts` is stamped before the push, so it is always `<= now`.
#[inline]
fn delay_nanos(send_ts: Instant, now: Instant) -> u64 {
    now.duration_since(send_ts).as_nanos() as u64
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

fn emit_sent(
    id: u32,
    msg_id: u64,
    sent_at: Option<Instant>,
    log: Option<String>,
    queue_len: usize,
) {
    send_channel_event(ChannelEvent::WrapMessageSent {
        id,
        msg_id,
        log,
        timestamp: crate::channels::anchor_first_msg(msg_id, sent_at),
        queue_len,
    });
}

fn emit_received(
    id: u32,
    msg_id: u64,
    now: Option<Instant>,
    queue_len: usize,
    delay_nanos: Option<u64>,
) {
    send_channel_event(ChannelEvent::WrapMessageReceived {
        id,
        msg_id,
        timestamp: now,
        queue_len,
        delay_nanos,
    });
}

/// Error returned by [`Sender::try_send`] and [`UnboundedSender::unbounded_send`].
///
/// Stands in for [`futures_channel::mpsc::TrySendError`], which cannot be rebuilt
/// around `T` once the payload is unwrapped; the API is the same.
pub struct TrySendError<T>(mpsc::TrySendError<Payload<T>>);

impl<T> TrySendError<T> {
    pub fn is_full(&self) -> bool {
        self.0.is_full()
    }

    pub fn is_disconnected(&self) -> bool {
        self.0.is_disconnected()
    }

    pub fn into_inner(self) -> T {
        self.0.into_inner().2
    }

    pub fn into_send_error(self) -> SendError {
        self.0.into_send_error()
    }
}

impl<T> std::fmt::Debug for TrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> std::fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: 'static> std::error::Error for TrySendError<T> {}

/// Shared state of every sender wrapper for one channel instance.
struct SendSide<T> {
    id: u32,
    sender_count: Arc<AtomicUsize>,
    next_id: Arc<AtomicU64>,
    depth: Arc<AtomicUsize>,
    closed: Arc<AtomicBool>,
    log_fn: Option<fn(&T) -> String>,
}

impl<T> SendSide<T> {
    /// Stamps `msg` and reserves its depth slot; the caller pushes the payload
    /// and either emits via [`Self::sent`] or rolls back via [`Self::failed`].
    fn prepare(&self, msg: T) -> (Payload<T>, Option<String>, usize) {
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let sent_at = sample_stamp();
        let queue_len = self.depth.fetch_add(1, Ordering::Relaxed) + 1;
        ((msg_id, sent_at, msg), log, queue_len)
    }

    fn sent(&self, msg_id: u64, sent_at: Option<Instant>, log: Option<String>, queue_len: usize) {
        emit_sent(self.id, msg_id, sent_at, log, queue_len);
    }

    fn failed(&self) {
        self.depth.fetch_sub(1, Ordering::Relaxed);
    }

    fn clone_handle(&self) -> Self {
        self.sender_count.fetch_add(1, Ordering::Relaxed);
        Self {
            id: self.id,
            sender_count: Arc::clone(&self.sender_count),
            next_id: Arc::clone(&self.next_id),
            depth: Arc::clone(&self.depth),
            closed: Arc::clone(&self.closed),
            log_fn: self.log_fn,
        }
    }

    fn drop_handle(&self) {
        if self.sender_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            let remaining = self.depth.load(Ordering::Relaxed);
            crate::channels::mark_closed(&self.closed, self.id, remaining);
        }
    }
}

/// Instrumented bounded [`futures_channel::mpsc::Sender`] wrapper.
pub struct Sender<T> {
    inner: mpsc::Sender<Payload<T>>,
    side: SendSide<T>,
}

impl<T> Sender<T> {
    pub fn try_send(&mut self, msg: T) -> Result<(), TrySendError<T>> {
        let ((msg_id, sent_at, msg), log, queue_len) = self.side.prepare(msg);
        match self.inner.try_send((msg_id, sent_at, msg)) {
            Ok(()) => {
                self.side.sent(msg_id, sent_at, log, queue_len);
                Ok(())
            }
            Err(e) => {
                self.side.failed();
                Err(TrySendError(e))
            }
        }
    }

    pub fn start_send(&mut self, msg: T) -> Result<(), SendError> {
        self.try_send(msg).map_err(TrySendError::into_send_error)
    }

    pub fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        self.inner.poll_ready(cx)
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    pub fn close_channel(&mut self) {
        self.inner.close_channel();
    }

    pub fn disconnect(&mut self) {
        self.inner.disconnect();
    }

    pub fn same_receiver(&self, other: &Self) -> bool {
        self.inner.same_receiver(&other.inner)
    }

    pub fn is_connected_to(&self, receiver: &Receiver<T>) -> bool {
        self.inner.is_connected_to(&receiver.inner)
    }

    pub fn hash_receiver<H>(&self, hasher: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.inner.hash_receiver(hasher);
    }
}

impl<T> Sink<T> for Sender<T> {
    type Error = SendError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        self.inner.poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, msg: T) -> Result<(), SendError> {
        Sender::start_send(&mut self, msg)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            side: self.side.clone_handle(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.side.drop_handle();
    }
}

/// Instrumented bounded [`futures_channel::mpsc::Receiver`] wrapper (single consumer).
pub struct Receiver<T> {
    inner: mpsc::Receiver<Payload<T>>,
    id: u32,
    depth: Arc<AtomicUsize>,
    closed: Arc<AtomicBool>,
}

impl<T> Receiver<T> {
    fn on_received(&self, msg_id: u64, send_ts: Option<Instant>) {
        let (now, delay) = recv_stamp(send_ts);
        let queue_len = self.depth.fetch_sub(1, Ordering::Relaxed) - 1;
        emit_received(self.id, msg_id, now, queue_len, delay);
    }

    /// No `Closed` emit here: remaining messages still drain after `close()`, and
    /// the state machine treats `Closed` as terminal. The drop impl emits it.
    pub fn close(&mut self) {
        self.inner.close();
    }

    pub async fn recv(&mut self) -> Result<T, RecvError> {
        let (msg_id, send_ts, msg) = self.inner.recv().await?;
        self.on_received(msg_id, send_ts);
        Ok(msg)
    }

    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let (msg_id, send_ts, msg) = self.inner.try_recv()?;
        self.on_received(msg_id, send_ts);
        Ok(msg)
    }

    #[deprecated(note = "please use `try_recv` instead")]
    #[allow(deprecated)]
    pub fn try_next(&mut self) -> Result<Option<T>, TryRecvError> {
        match self.inner.try_next()? {
            Some((msg_id, send_ts, msg)) => {
                self.on_received(msg_id, send_ts);
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some((msg_id, send_ts, msg))) => {
                self.on_received(msg_id, send_ts);
                Poll::Ready(Some(msg))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> FusedStream for Receiver<T> {
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        // Close first so a send racing this drop fails instead of landing after
        // the remaining count is read.
        self.inner.close();
        crate::channels::mark_closed(&self.closed, self.id, self.inner.size_hint().0);
    }
}

/// Instrumented [`futures_channel::mpsc::UnboundedSender`] wrapper.
pub struct UnboundedSender<T> {
    inner: mpsc::UnboundedSender<Payload<T>>,
    side: SendSide<T>,
}

impl<T> UnboundedSender<T> {
    pub fn unbounded_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        let ((msg_id, sent_at, msg), log, queue_len) = self.side.prepare(msg);
        match self.inner.unbounded_send((msg_id, sent_at, msg)) {
            Ok(()) => {
                self.side.sent(msg_id, sent_at, log, queue_len);
                Ok(())
            }
            Err(e) => {
                self.side.failed();
                Err(TrySendError(e))
            }
        }
    }

    pub fn start_send(&mut self, msg: T) -> Result<(), SendError> {
        self.unbounded_send(msg)
            .map_err(TrySendError::into_send_error)
    }

    pub fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        self.inner.poll_ready(cx)
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    pub fn close_channel(&self) {
        self.inner.close_channel();
    }

    pub fn disconnect(&mut self) {
        self.inner.disconnect();
    }

    pub fn same_receiver(&self, other: &Self) -> bool {
        self.inner.same_receiver(&other.inner)
    }

    pub fn is_connected_to(&self, receiver: &UnboundedReceiver<T>) -> bool {
        self.inner.is_connected_to(&receiver.inner)
    }

    pub fn hash_receiver<H>(&self, hasher: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.inner.hash_receiver(hasher);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<T> Sink<T> for UnboundedSender<T> {
    type Error = SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        self.inner.poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, msg: T) -> Result<(), SendError> {
        UnboundedSender::start_send(&mut self, msg)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

impl<T> Sink<T> for &UnboundedSender<T> {
    type Error = SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        self.inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, msg: T) -> Result<(), SendError> {
        self.unbounded_send(msg)
            .map_err(TrySendError::into_send_error)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        self.close_channel();
        Poll::Ready(Ok(()))
    }
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            side: self.side.clone_handle(),
        }
    }
}

impl<T> Drop for UnboundedSender<T> {
    fn drop(&mut self) {
        self.side.drop_handle();
    }
}

/// Instrumented [`futures_channel::mpsc::UnboundedReceiver`] wrapper (single consumer).
pub struct UnboundedReceiver<T> {
    inner: mpsc::UnboundedReceiver<Payload<T>>,
    id: u32,
    depth: Arc<AtomicUsize>,
    closed: Arc<AtomicBool>,
}

impl<T> UnboundedReceiver<T> {
    fn on_received(&self, msg_id: u64, send_ts: Option<Instant>) {
        let (now, delay) = recv_stamp(send_ts);
        let queue_len = self.depth.fetch_sub(1, Ordering::Relaxed) - 1;
        emit_received(self.id, msg_id, now, queue_len, delay);
    }

    /// No `Closed` emit here: remaining messages still drain after `close()`, and
    /// the state machine treats `Closed` as terminal. The drop impl emits it.
    pub fn close(&mut self) {
        self.inner.close();
    }

    pub async fn recv(&mut self) -> Result<T, RecvError> {
        let (msg_id, send_ts, msg) = self.inner.recv().await?;
        self.on_received(msg_id, send_ts);
        Ok(msg)
    }

    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let (msg_id, send_ts, msg) = self.inner.try_recv()?;
        self.on_received(msg_id, send_ts);
        Ok(msg)
    }

    #[deprecated(note = "please use `try_recv` instead")]
    #[allow(deprecated)]
    pub fn try_next(&mut self) -> Result<Option<T>, TryRecvError> {
        match self.inner.try_next()? {
            Some((msg_id, send_ts, msg)) => {
                self.on_received(msg_id, send_ts);
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }
}

impl<T> Stream for UnboundedReceiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some((msg_id, send_ts, msg))) => {
                self.on_received(msg_id, send_ts);
                Poll::Ready(Some(msg))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> FusedStream for UnboundedReceiver<T> {
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}

impl<T> Drop for UnboundedReceiver<T> {
    fn drop(&mut self) {
        // Close first so a send racing this drop fails instead of landing after
        // the remaining count is read.
        self.inner.close();
        crate::channels::mark_closed(&self.closed, self.id, self.inner.size_hint().0);
    }
}

// futures' endpoints implement `Debug` for any `T`, so the wrappers delegate
// unconditionally too.
macro_rules! impl_debug_via_inner {
    ($($ty:ident),+) => {$(
        impl<T> std::fmt::Debug for $ty<T> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($ty))
                    .field("inner", &self.inner)
                    .finish_non_exhaustive()
            }
        }
    )+};
}

impl_debug_via_inner!(Sender, Receiver, UnboundedSender, UnboundedReceiver);

fn send_side<T>(id: u32, iter: bool, log_fn: Option<fn(&T) -> String>) -> SendSide<T> {
    // Aggregated instances share one msg-id sequence so ids stay unique
    // within the entry; iter-mode instances keep a local counter.
    let next_id = if iter {
        Arc::new(AtomicU64::new(0))
    } else {
        crate::channels::entry_msg_counter(id)
    };
    SendSide {
        id,
        sender_count: Arc::new(AtomicUsize::new(1)),
        next_id,
        depth: Arc::new(AtomicUsize::new(0)),
        closed: Arc::new(AtomicBool::new(false)),
        log_fn,
    }
}

fn build_bounded<T>(
    source: &'static str,
    label: Option<String>,
    capacity: Option<usize>,
    log_fn: Option<fn(&T) -> String>,
    iter: bool,
) -> (Sender<T>, Receiver<T>) {
    let Some(capacity) = capacity else {
        panic!("Capacity is required for bounded futures channels, because they don't expose their capacity in a public API");
    };
    let id = register_channel::<T>(source, label, ChannelType::Bounded(capacity), iter);
    // Rebuild to carry `(msg_id, send_ts, T)`; the caller's channel is discarded
    // (the wrapper is inline-only).
    let (tx, rx) = mpsc::channel::<Payload<T>>(capacity);
    let side = send_side(id, iter, log_fn);
    let receiver = Receiver {
        inner: rx,
        id,
        depth: Arc::clone(&side.depth),
        closed: Arc::clone(&side.closed),
    };
    (Sender { inner: tx, side }, receiver)
}

fn build_unbounded<T>(
    source: &'static str,
    label: Option<String>,
    log_fn: Option<fn(&T) -> String>,
    iter: bool,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let id = register_channel::<T>(source, label, ChannelType::Unbounded, iter);
    let (tx, rx) = mpsc::unbounded::<Payload<T>>();
    let side = send_side(id, iter, log_fn);
    let receiver = UnboundedReceiver {
        inner: rx,
        id,
        depth: Arc::clone(&side.depth),
        closed: Arc::clone(&side.closed),
    };
    (UnboundedSender { inner: tx, side }, receiver)
}

impl<T: Send + 'static> InstrumentChannelWrap for (mpsc::Sender<T>, mpsc::Receiver<T>) {
    type Output = (Sender<T>, Receiver<T>);
    fn instrument_wrap(
        self,
        source: &'static str,
        label: Option<String>,
        capacity: Option<usize>,
        iter: bool,
    ) -> Self::Output {
        build_bounded(source, label, capacity, None, iter)
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
        iter: bool,
    ) -> Self::Output {
        build_unbounded(source, label, None, iter)
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
        capacity: Option<usize>,
        iter: bool,
    ) -> Self::Output {
        let log_fn: fn(&T) -> String = |m| crate::output_on::format_debug_truncated(m);
        build_bounded(source, label, capacity, Some(log_fn), iter)
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
        iter: bool,
    ) -> Self::Output {
        let log_fn: fn(&T) -> String = |m| crate::output_on::format_debug_truncated(m);
        build_unbounded(source, label, Some(log_fn), iter)
    }
}

#[cfg(test)]
mod tests {
    use crate::channels::wrapper::ftc_wrap::{
        build_bounded, build_unbounded, Receiver, Sender, UnboundedReceiver, UnboundedSender,
    };
    use futures_channel::mpsc::RecvError;
    use futures_core::stream::{FusedStream, Stream};
    use futures_util::{SinkExt, StreamExt};

    fn bounded<T: Send + 'static>(capacity: usize) -> (Sender<T>, Receiver<T>) {
        build_bounded::<T>("test", None, Some(capacity), None, false)
    }

    fn unbounded<T: Send + 'static>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
        build_unbounded::<T>("test", None, None, false)
    }

    #[test]
    fn send_only_payload_keeps_endpoints_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        // `Cell<u8>` is `Send` but not `Sync`, matching payloads like `Box<dyn Trait + Send>`.
        type P = std::cell::Cell<u8>;
        assert_send_sync::<Sender<P>>();
        assert_send_sync::<Receiver<P>>();
        assert_send_sync::<UnboundedSender<P>>();
        assert_send_sync::<UnboundedReceiver<P>>();
    }

    #[tokio::test]
    async fn sink_and_stream_roundtrip() {
        let (mut tx, mut rx) = bounded::<u32>(2);
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        assert_eq!(rx.size_hint().0, 2);
        assert_eq!(rx.next().await, Some(1));
        assert_eq!(rx.next().await, Some(2));
        drop(tx);
        assert_eq!(rx.next().await, None);
        assert!(rx.is_terminated());

        let (mut tx, mut rx) = unbounded::<u32>();
        tx.send(7).await.unwrap();
        (&tx).send(8).await.unwrap();
        assert_eq!(tx.len(), 2);
        assert_eq!(rx.next().await, Some(7));
        assert_eq!(rx.next().await, Some(8));
        assert!(tx.is_empty());
    }

    #[tokio::test]
    async fn recv_returns_messages_then_error_when_closed() {
        let (mut tx, mut rx) = bounded::<u32>(1);
        tx.send(1).await.unwrap();
        assert_eq!(rx.recv().await, Ok(1));
        drop(tx);
        assert_eq!(rx.recv().await, Err(RecvError));

        let (tx, mut rx) = unbounded::<u32>();
        tx.unbounded_send(2).unwrap();
        assert_eq!(rx.recv().await, Ok(2));
        drop(tx);
        assert_eq!(rx.recv().await, Err(RecvError));
    }

    #[test]
    fn try_send_full_error_returns_value() {
        // A bounded futures channel holds `capacity + 1` per sender before it is full.
        let (mut tx, mut rx) = bounded::<u32>(0);
        tx.try_send(1).unwrap();
        let err = tx.try_send(2).unwrap_err();
        assert!(err.is_full());
        assert!(!err.is_disconnected());
        assert_eq!(err.into_inner(), 2);
        assert_eq!(rx.try_recv().unwrap(), 1);
        assert!(rx.try_recv().unwrap_err().is_empty());
    }

    #[test]
    fn send_after_receiver_drop_is_disconnected() {
        let (tx, rx) = unbounded::<u32>();
        drop(rx);
        assert!(tx.is_closed());
        let err = tx.unbounded_send(5).unwrap_err();
        assert!(err.is_disconnected());
        assert!(err.into_send_error().is_disconnected());

        let (mut tx, rx) = bounded::<u32>(1);
        drop(rx);
        assert!(tx.try_send(5).unwrap_err().is_disconnected());
        assert!(tx.start_send(6).unwrap_err().is_disconnected());
    }

    #[test]
    fn close_stops_sends_but_drains() {
        let (mut tx, mut rx) = bounded::<u32>(4);
        tx.try_send(1).unwrap();
        rx.close();
        assert!(tx.is_closed());
        assert!(tx.try_send(2).unwrap_err().is_disconnected());
        assert_eq!(rx.try_recv().unwrap(), 1);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn same_receiver_across_clones() {
        let (tx_a, rx_a) = bounded::<u32>(4);
        let (tx_b, _rx_b) = bounded::<u32>(4);
        assert!(tx_a.same_receiver(&tx_a.clone()));
        assert!(!tx_a.same_receiver(&tx_b));
        assert!(tx_a.is_connected_to(&rx_a));

        let (utx_a, urx_a) = unbounded::<u32>();
        let (utx_b, _urx_b) = unbounded::<u32>();
        assert!(utx_a.same_receiver(&utx_a.clone()));
        assert!(!utx_a.same_receiver(&utx_b));
        assert!(utx_a.is_connected_to(&urx_a));
    }
}
