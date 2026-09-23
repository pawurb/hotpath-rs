//! Endpoint-wrapping Tokio `oneshot` channel instrumentation for the `channel!` macro.
//!
//! Wraps the `Sender`/`Receiver` endpoints directly: `send` and the receive future hit
//! the real channel and the only added cost is a non-blocking event emit.
//!
//! The inner channel carries `(msg_id, send_ts, T)`. A oneshot moves exactly one value, so
//! `msg_id` is drawn once per instance from the call site's shared sequence (aggregated
//! entries) or is `0` (`iter = true`). `send_ts` is stamped before the send so the
//! reported delay is non-negative; both fields are internal and the public API still uses
//! `T`.
//!
//! Queue depth is exact by construction: `1` after a successful `send`, `0` once the
//! receiver takes the value. A successful `send` also emits `Notified`, the oneshot's
//! terminal state. `Closed` is emitted only when the channel tears down *without*
//! delivering: the sender dropped unsent, or the receiver dropped before taking the
//! value. In the latter case the value is also reported as `Abandoned` so the call
//! site's counts-derived depth does not keep a phantom message.
//!
//! The wrapper rebuilds the inner channel, so the `channel!` expression must be
//! constructed inline; the endpoints you pass in are discarded.
//!
//! Returns [`Sender`]/[`Receiver`], re-exported as `hotpath::wrap::tokio::sync::oneshot::*`.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::sync::oneshot;
use tokio::sync::oneshot::error::{RecvError, TryRecvError};

use crate::channels::{
    register_channel, sample_stamp, send_channel_event, ChannelEvent, ChannelType, Instant,
    InstrumentChannelWrap, InstrumentChannelWrapLog,
};

type Payload<T> = (u64, Option<Instant>, T);

/// A `Some` payload stamp means the message is sampled: stamp `now`, compute the delay.
#[inline]
fn recv_stamp(send_ts: Option<Instant>) -> (Option<Instant>, Option<u64>) {
    match send_ts {
        Some(ts) => {
            let now = Instant::now();
            (Some(now), Some(now.duration_since(ts).as_nanos() as u64))
        }
        None => (None, None),
    }
}

/// Instrumented [`tokio::sync::oneshot::Sender`] wrapper.
pub struct Sender<T> {
    /// `None` after `send` consumed it.
    inner: Option<oneshot::Sender<Payload<T>>>,
    id: u32,
    next_id: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
    /// Set by a successful `send`; the drop impl then leaves teardown to the receiver.
    sent: bool,
    log_fn: Option<fn(&T) -> String>,
}

impl<T> Sender<T> {
    fn inner(&mut self) -> &mut oneshot::Sender<Payload<T>> {
        self.inner
            .as_mut()
            .expect("oneshot sender endpoint is only taken by `send`")
    }

    pub fn send(mut self, msg: T) -> Result<(), T> {
        let inner = self
            .inner
            .take()
            .expect("oneshot sender endpoint is only taken by `send`");
        let log = self.log_fn.map(|f| f(&msg));
        let msg_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let sent_at = sample_stamp();
        if let Err((_, _, msg)) = inner.send((msg_id, sent_at, msg)) {
            return Err(msg);
        }
        self.sent = true;
        send_channel_event(ChannelEvent::Notified { id: self.id });
        send_channel_event(ChannelEvent::WrapMessageSent {
            id: self.id,
            msg_id,
            log,
            timestamp: crate::channels::anchor_first_msg(msg_id, sent_at),
            queue_len: 1,
        });
        Ok(())
    }

    pub async fn closed(&mut self) {
        self.inner().closed().await
    }

    pub fn is_closed(&self) -> bool {
        self.inner.as_ref().is_none_or(|inner| inner.is_closed())
    }

    pub fn poll_closed(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        self.inner().poll_closed(cx)
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // After a successful `send` the receiver settles the state.
        if !self.sent {
            crate::channels::mark_closed(&self.closed, self.id, 0);
        }
    }
}

/// Instrumented [`tokio::sync::oneshot::Receiver`] wrapper.
///
/// Implements [`Future`] like the tokio receiver, so `rx.await` works unchanged.
pub struct Receiver<T> {
    /// `None` after `blocking_recv` consumed it.
    inner: Option<oneshot::Receiver<Payload<T>>>,
    id: u32,
    closed: Arc<AtomicBool>,
    received: bool,
}

impl<T> Receiver<T> {
    fn inner(&mut self) -> &mut oneshot::Receiver<Payload<T>> {
        self.inner
            .as_mut()
            .expect("oneshot receiver endpoint is only taken by `blocking_recv`")
    }

    fn on_received(&mut self, msg_id: u64, send_ts: Option<Instant>) {
        let (now, delay_nanos) = recv_stamp(send_ts);
        self.received = true;
        send_channel_event(ChannelEvent::WrapMessageReceived {
            id: self.id,
            msg_id,
            timestamp: now,
            queue_len: 0,
            delay_nanos,
        });
    }

    /// No `Closed` emit here: a value sent before `close()` can still be taken
    /// with `try_recv`. The drop impl settles the state.
    pub fn close(&mut self) {
        self.inner().close();
    }

    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let (msg_id, send_ts, msg) = self.inner().try_recv()?;
        self.on_received(msg_id, send_ts);
        Ok(msg)
    }

    /// Event emission is a sync crossbeam send, so it is safe off-runtime; like
    /// tokio's `blocking_recv` this panics when called from an async context.
    pub fn blocking_recv(mut self) -> Result<T, RecvError> {
        let inner = self
            .inner
            .take()
            .expect("oneshot receiver endpoint is only taken by `blocking_recv`");
        let (msg_id, send_ts, msg) = inner.blocking_recv()?;
        self.on_received(msg_id, send_ts);
        Ok(msg)
    }

    pub fn is_terminated(&self) -> bool {
        self.inner
            .as_ref()
            .is_none_or(|inner| inner.is_terminated())
    }

    pub fn is_empty(&self) -> bool {
        self.inner.as_ref().is_none_or(|inner| inner.is_empty())
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match Pin::new(this.inner()).poll(cx) {
            Poll::Ready(Ok((msg_id, send_ts, msg))) => {
                this.on_received(msg_id, send_ts);
                Poll::Ready(Ok(msg))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if self.received {
            return;
        }
        // `close()` first so a send racing this drop fails; `try_recv` then
        // tells whether a value was already in flight.
        let pending = match self.inner.as_mut() {
            Some(inner) => {
                inner.close();
                inner.try_recv().is_ok()
            }
            None => false,
        };
        if pending {
            // The sender emitted no `Closed` after delivering, so this teardown
            // must, then retire the value nobody will take.
            self.closed.store(true, Ordering::Release);
            send_channel_event(ChannelEvent::Closed { id: self.id });
            send_channel_event(ChannelEvent::Abandoned {
                id: self.id,
                count: 1,
            });
        } else {
            crate::channels::mark_closed(&self.closed, self.id, 0);
        }
    }
}

// Tokio's oneshot endpoints derive `Debug` under `T: Debug`, so the wrappers do too.
impl<T: std::fmt::Debug> std::fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender")
            .field("inner", &self.inner)
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Receiver")
            .field("inner", &self.inner)
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

fn build<T>(
    source: &'static str,
    label: Option<String>,
    log_fn: Option<fn(&T) -> String>,
    iter: bool,
) -> (Sender<T>, Receiver<T>) {
    let id = register_channel::<T>(source, label, ChannelType::Oneshot, iter);
    // Rebuild to carry `(msg_id, send_ts, T)`; the caller's channel is discarded
    // (the wrapper is inline-only).
    let (tx, rx) = oneshot::channel::<Payload<T>>();
    let closed = Arc::new(AtomicBool::new(false));
    // Aggregated instances share one msg-id sequence so ids stay unique
    // within the entry; iter-mode instances keep a local counter.
    let next_id = if iter {
        Arc::new(AtomicU64::new(0))
    } else {
        crate::channels::entry_msg_counter(id)
    };
    let sender = Sender {
        inner: Some(tx),
        id,
        next_id,
        closed: Arc::clone(&closed),
        sent: false,
        log_fn,
    };
    let receiver = Receiver {
        inner: Some(rx),
        id,
        closed,
        received: false,
    };
    (sender, receiver)
}

impl<T: Send + 'static> InstrumentChannelWrap for (oneshot::Sender<T>, oneshot::Receiver<T>) {
    type Output = (Sender<T>, Receiver<T>);
    fn instrument_wrap(
        self,
        source: &'static str,
        label: Option<String>,
        _capacity: Option<usize>,
        iter: bool,
    ) -> Self::Output {
        build(source, label, None, iter)
    }
}

impl<T: Send + std::fmt::Debug + 'static> InstrumentChannelWrapLog
    for (oneshot::Sender<T>, oneshot::Receiver<T>)
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
        build(source, label, Some(log_fn), iter)
    }
}

#[cfg(test)]
mod tests {
    use crate::channels::wrapper::tokio_oneshot_wrap::{build, Receiver, Sender};

    fn oneshot<T: Send + 'static>() -> (Sender<T>, Receiver<T>) {
        build::<T>("test", None, None, false)
    }

    #[test]
    fn send_only_payload_keeps_endpoints_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        // `Cell<u8>` is `Send` but not `Sync`, matching payloads like `Box<dyn Trait + Send>`.
        type P = std::cell::Cell<u8>;
        assert_send_sync::<Sender<P>>();
        assert_send_sync::<Receiver<P>>();
    }

    #[test]
    fn try_recv_and_blocking_recv() {
        let (tx, mut rx) = oneshot::<u32>();
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        tx.send(1).unwrap();
        assert_eq!(rx.try_recv().unwrap(), 1);

        let (tx, rx) = oneshot::<u32>();
        tx.send(2).unwrap();
        assert_eq!(rx.blocking_recv().unwrap(), 2);
    }

    #[test]
    fn send_fails_after_receiver_drop_and_returns_value() {
        let (tx, rx) = oneshot::<String>();
        drop(rx);
        assert!(tx.is_closed());
        assert_eq!(tx.send("lost".to_string()), Err("lost".to_string()));
    }

    #[test]
    fn close_stops_send_but_keeps_pending_value() {
        let (tx, mut rx) = oneshot::<u32>();
        tx.send(3).unwrap();
        rx.close();
        assert_eq!(rx.try_recv().unwrap(), 3);

        let (tx, mut rx) = oneshot::<u32>();
        rx.close();
        assert!(tx.send(4).is_err());
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed)
        ));
    }
}
