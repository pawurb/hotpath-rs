//! Endpoint-wrapping crossbeam channel instrumentation (`channel!(..., wrap = true)`).
//!
//! Unlike the forwarder-proxy wrapping in [`crate::channels::wrapper::crossbeam`],
//! this wraps the `Sender`/`Receiver` endpoints directly. No extra thread or proxy
//! channel is inserted, so send/recv happen on the real channel and the queue depth
//! reported (`queue_len`) is a snapshot of the channel's length taken right after each
//! operation. It is exact for single-threaded use; with concurrent senders/receivers it
//! may skew by the number of operations other threads complete in that window.
//!
//! Returned types are [`Sender`]/[`Receiver`], re-exported as
//! `hotpath::wrap::crossbeam::{Sender, Receiver}`.

use std::sync::atomic::{AtomicUsize, Ordering};
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
    inner: InnerSender<T>,
    id: u32,
    sender_count: Arc<AtomicUsize>,
    log_fn: Option<fn(&T) -> String>,
}

impl<T> Sender<T> {
    fn emit_sent(&self, log: Option<String>) {
        send_channel_event(ChannelEvent::WrapMessageSent {
            id: self.id,
            log,
            timestamp: Instant::now(),
            queue_len: self.inner.len(),
        });
    }

    pub fn send(&self, msg: T) -> Result<(), SendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        self.inner.send(msg)?;
        self.emit_sent(log);
        Ok(())
    }

    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        self.inner.try_send(msg)?;
        self.emit_sent(log);
        Ok(())
    }

    pub fn send_timeout(
        &self,
        msg: T,
        timeout: std::time::Duration,
    ) -> Result<(), SendTimeoutError<T>> {
        let log = self.log_fn.map(|f| f(&msg));
        self.inner.send_timeout(msg, timeout)?;
        self.emit_sent(log);
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
    inner: InnerReceiver<T>,
    id: u32,
    receiver_count: Arc<AtomicUsize>,
}

impl<T> Receiver<T> {
    fn on_received(&self) {
        send_channel_event(ChannelEvent::WrapMessageReceived {
            id: self.id,
            timestamp: Instant::now(),
            queue_len: self.inner.len(),
        });
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        let msg = self.inner.recv()?;
        self.on_received();
        Ok(msg)
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let msg = self.inner.try_recv()?;
        self.on_received();
        Ok(msg)
    }

    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Result<T, RecvTimeoutError> {
        let msg = self.inner.recv_timeout(timeout)?;
        self.on_received();
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
    let (tx, rx) = inner;
    let id = register_channel_wrap::<T>(source, label, channel_type(&tx));
    let sender = Sender {
        inner: tx,
        id,
        sender_count: Arc::new(AtomicUsize::new(1)),
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
