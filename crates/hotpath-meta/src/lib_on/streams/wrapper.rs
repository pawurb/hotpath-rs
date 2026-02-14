use crate::data_flow::next_data_flow_id;
use crate::output::format_debug_truncated;
use crate::streams::{init_streams_state, StreamEvent};
use crossbeam_channel::Sender as CbSender;
use futures_util::Stream;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

pin_project! {
    /// Wrapper around a `Stream` that instruments it with statistics collection.
    ///
    /// This struct implements the `Stream` trait and forwards all calls to the inner stream
    /// while recording statistics about yielded items.
    pub struct InstrumentedStream<S> {
        #[pin]
        inner: S,
        stats_tx: CbSender<StreamEvent>,
        id: u64,
    }
}

impl<S> InstrumentedStream<S> {
    pub(crate) fn new(stream: S, source: &'static str, label: Option<String>) -> Self
    where
        S: Stream,
    {
        let state = init_streams_state();
        let id = next_data_flow_id();

        let _ = state.event_tx.send(StreamEvent::Created {
            id,
            source,
            display_label: label,
            type_name: std::any::type_name::<S::Item>(),
            type_size: std::mem::size_of::<S::Item>(),
        });

        Self {
            inner: stream,
            stats_tx: state.event_tx.clone(),
            id,
        }
    }
}

impl<S: Stream> Stream for InstrumentedStream<S> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        match this.inner.poll_next(cx) {
            Poll::Ready(Some(item)) => {
                let _ = this.stats_tx.send(StreamEvent::Yielded {
                    id: *this.id,
                    log: None,
                    timestamp: Instant::now(),
                });
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                let _ = this.stats_tx.send(StreamEvent::Completed { id: *this.id });
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

pin_project! {
    /// Wrapper around a `Stream` that instruments it with message logging enabled.
    ///
    /// This variant captures the Debug representation of yielded items.
    pub struct InstrumentedStreamLog<S> {
        #[pin]
        inner: S,
        stats_tx: CbSender<StreamEvent>,
        id: u64,
    }
}

impl<S> InstrumentedStreamLog<S> {
    pub(crate) fn new(stream: S, source: &'static str, label: Option<String>) -> Self
    where
        S: Stream,
    {
        let state = init_streams_state();
        let id = next_data_flow_id();

        let _ = state.event_tx.send(StreamEvent::Created {
            id,
            source,
            display_label: label,
            type_name: std::any::type_name::<S::Item>(),
            type_size: std::mem::size_of::<S::Item>(),
        });

        Self {
            inner: stream,
            stats_tx: state.event_tx.clone(),
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
        let this = self.project();

        match this.inner.poll_next(cx) {
            Poll::Ready(Some(item)) => {
                let log_msg = format_debug_truncated(&item);
                let _ = this.stats_tx.send(StreamEvent::Yielded {
                    id: *this.id,
                    log: Some(log_msg),
                    timestamp: Instant::now(),
                });
                Poll::Ready(Some(item))
            }
            Poll::Ready(None) => {
                let _ = this.stats_tx.send(StreamEvent::Completed { id: *this.id });
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
