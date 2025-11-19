use crate::channels::{init_streams_state, StreamEvent, STREAM_ID_COUNTER};
use crossbeam_channel::Sender as CbSender;
use futures_util::Stream;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};
use std::time::Instant;

/// Wrapper around a `Stream` that instruments it with statistics collection.
///
/// This struct implements the `Stream` trait and forwards all calls to the inner stream
/// while recording statistics about yielded items.
pub struct InstrumentedStream<S> {
    inner: S,
    stats_tx: CbSender<StreamEvent>,
    id: u64,
}

impl<S> InstrumentedStream<S> {
    /// Create a new instrumented stream wrapper.
    ///
    /// # Parameters
    /// - `stream`: The underlying stream to instrument
    /// - `source`: Source location (file:line) for identification
    /// - `label`: Optional custom label
    pub(crate) fn new(stream: S, source: &'static str, label: Option<String>) -> Self
    where
        S: Stream,
    {
        let (stats_tx, _) = init_streams_state();
        let id = STREAM_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        // Send stream creation event
        let _ = stats_tx.send(StreamEvent::Created {
            id,
            source,
            display_label: label,
            type_name: std::any::type_name::<S::Item>(),
            type_size: std::mem::size_of::<S::Item>(),
        });

        Self {
            inner: stream,
            stats_tx: stats_tx.clone(),
            id,
        }
    }
}

impl<S: Stream> Stream for InstrumentedStream<S> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // SAFETY: We need to project through the Pin to access the inner stream.
        // This is safe because we don't move the inner stream, we just get a mutable reference.
        // The outer InstrumentedStream being pinned ensures the inner stream stays pinned.
        let this = unsafe { self.get_unchecked_mut() };
        let inner = unsafe { Pin::new_unchecked(&mut this.inner) };

        match inner.poll_next(cx) {
            Poll::Ready(Some(item)) => {
                let _ = this.stats_tx.send(StreamEvent::Yielded {
                    id: this.id,
                    log: None,
                    timestamp: Instant::now(),
                });
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                let _ = this.stats_tx.send(StreamEvent::Completed { id: this.id });
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Wrapper around a `Stream` that instruments it with message logging enabled.
///
/// This variant captures the Debug representation of yielded items.
pub struct InstrumentedStreamLog<S> {
    inner: S,
    stats_tx: CbSender<StreamEvent>,
    id: u64,
}

impl<S> InstrumentedStreamLog<S> {
    /// Create a new instrumented stream wrapper with logging.
    pub(crate) fn new(stream: S, source: &'static str, label: Option<String>) -> Self
    where
        S: Stream,
    {
        let (stats_tx, _) = init_streams_state();
        let id = STREAM_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        // Send stream creation event
        let _ = stats_tx.send(StreamEvent::Created {
            id,
            source,
            display_label: label,
            type_name: std::any::type_name::<S::Item>(),
            type_size: std::mem::size_of::<S::Item>(),
        });

        Self {
            inner: stream,
            stats_tx: stats_tx.clone(),
            id,
        }
    }
}

impl<S: Stream> Stream for InstrumentedStreamLog<S>
where
    S::Item: std::fmt::Debug,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // SAFETY: Same as above - we're projecting through Pin without moving
        let this = unsafe { self.get_unchecked_mut() };
        let inner = unsafe { Pin::new_unchecked(&mut this.inner) };

        match inner.poll_next(cx) {
            Poll::Ready(Some(item)) => {
                let log_msg = format!("{:?}", item);
                dbg!(&log_msg);
                let _ = this.stats_tx.send(StreamEvent::Yielded {
                    id: this.id,
                    log: Some(log_msg),
                    timestamp: Instant::now(),
                });
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                let _ = this.stats_tx.send(StreamEvent::Completed { id: this.id });
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
