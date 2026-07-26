//! Instrumented wrapper for byte-level I/O values.

use std::io::{IoSlice, IoSliceMut, Read, Write};

use crate::instant::Instant;
use crate::io::{
    cancel_op_stamp, elapsed_nanos, op_stamp, register_io, send_io_event, IoEvent, IoOpKind,
};

cfg_if::cfg_if! {
    if #[cfg(feature = "tokio")] {
        use pin_project_lite::pin_project;
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

        pin_project! {
            /// Instrumented wrapper around a byte-level I/O value.
            ///
            /// Not constructed directly - use the [`io!`](crate::io) macro.
            /// Delegates [`Read`], [`Write`], `AsyncRead`, and `AsyncWrite` to the
            /// wrapped value while recording operation counts, bytes, durations,
            /// and errors. The per-direction `*_op` slots hold the in-flight async
            /// operation state: `Some` while an operation spans `Pending` polls,
            /// with the inner `Option<Instant>` being the sampled start stamp.
            pub struct InstrumentedIo<T> {
                #[pin]
                inner: T,
                id: u32,
                read_op: Option<Option<Instant>>,
                write_op: Option<Option<Instant>>,
                flush_op: Option<Option<Instant>>,
                shutdown_op: Option<Option<Instant>>,
            }
        }
    } else {
        /// Instrumented wrapper around a byte-level I/O value.
        ///
        /// Not constructed directly - use the [`io!`](crate::io) macro.
        /// Delegates [`Read`] and [`Write`] to the wrapped value while recording
        /// operation counts, bytes, durations, and errors.
        pub struct InstrumentedIo<T> {
            inner: T,
            id: u32,
        }
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl<T> InstrumentedIo<T> {
    #[doc(hidden)]
    pub fn __new_instrumented(inner: T, source: &'static str, label: Option<String>) -> Self {
        let id = register_io::<T>(source, label);
        Self {
            inner,
            id,
            #[cfg(feature = "tokio")]
            read_op: None,
            #[cfg(feature = "tokio")]
            write_op: None,
            #[cfg(feature = "tokio")]
            flush_op: None,
            #[cfg(feature = "tokio")]
            shutdown_op: None,
        }
    }

    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

/// Records a completed synchronous operation. Retryable conditions
/// (`WouldBlock`, `Interrupted`) produce no event; other errors are counted.
/// Both roll back the sampling decision so the rate applies to completed
/// operations.
fn record_sync_op(
    id: u32,
    kind: IoOpKind,
    start: Option<Instant>,
    outcome: Result<u64, std::io::ErrorKind>,
) {
    match outcome {
        Ok(bytes) => send_io_event(IoEvent::Op {
            id,
            kind,
            bytes,
            duration_nanos: start.map(elapsed_nanos),
        }),
        Err(std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => cancel_op_stamp(),
        Err(_) => {
            cancel_op_stamp();
            send_io_event(IoEvent::Error { id, kind });
        }
    }
}

impl<T: Read> Read for InstrumentedIo<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let start = op_stamp();
        let result = self.inner.read(buf);
        record_sync_op(
            self.id,
            IoOpKind::Read,
            start,
            result.as_ref().map(|n| *n as u64).map_err(|e| e.kind()),
        );
        result
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> std::io::Result<usize> {
        let start = op_stamp();
        let result = self.inner.read_vectored(bufs);
        record_sync_op(
            self.id,
            IoOpKind::Read,
            start,
            result.as_ref().map(|n| *n as u64).map_err(|e| e.kind()),
        );
        result
    }
}

impl<T: Write> Write for InstrumentedIo<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let start = op_stamp();
        let result = self.inner.write(buf);
        record_sync_op(
            self.id,
            IoOpKind::Write,
            start,
            result.as_ref().map(|n| *n as u64).map_err(|e| e.kind()),
        );
        result
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> std::io::Result<usize> {
        let start = op_stamp();
        let result = self.inner.write_vectored(bufs);
        record_sync_op(
            self.id,
            IoOpKind::Write,
            start,
            result.as_ref().map(|n| *n as u64).map_err(|e| e.kind()),
        );
        result
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let start = op_stamp();
        let result = self.inner.flush();
        record_sync_op(
            self.id,
            IoOpKind::Flush,
            start,
            result.as_ref().map(|_| 0).map_err(|e| e.kind()),
        );
        result
    }
}

/// Stamps the start of an async operation on its first poll. Subsequent
/// `Pending` re-polls keep the original stamp, so the duration reported at
/// `Ready` spans the whole operation including suspended time.
#[cfg(feature = "tokio")]
#[inline]
fn begin_async_op(op: &mut Option<Option<Instant>>) {
    if op.is_none() {
        *op = Some(op_stamp());
    }
}

/// Resolves a delegated poll: `Ready(Ok)` emits an operation event with the
/// full first-poll-to-ready duration, `Ready(Err)` emits an error event, and
/// both clear the in-flight state so a later operation cannot inherit a stale
/// start stamp. If the operation's future is cancelled mid-`Pending` the slot
/// stays stamped and the next operation on the same direction resumes it,
/// reporting the time since the abandoned operation began.
#[cfg(feature = "tokio")]
fn finish_async_op<R>(
    op: &mut Option<Option<Instant>>,
    id: u32,
    kind: IoOpKind,
    poll: Poll<std::io::Result<R>>,
    bytes: impl FnOnce(&R) -> u64,
) -> Poll<std::io::Result<R>> {
    match &poll {
        Poll::Pending => {}
        Poll::Ready(Ok(value)) => {
            let duration_nanos = op.take().flatten().map(elapsed_nanos);
            send_io_event(IoEvent::Op {
                id,
                kind,
                bytes: bytes(value),
                duration_nanos,
            });
        }
        Poll::Ready(Err(_)) => {
            *op = None;
            send_io_event(IoEvent::Error { id, kind });
        }
    }
    poll
}

#[cfg(feature = "tokio")]
impl<T: AsyncRead> AsyncRead for InstrumentedIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.project();
        begin_async_op(this.read_op);
        let before = buf.filled().len();
        let poll = this.inner.poll_read(cx, buf);
        finish_async_op(this.read_op, *this.id, IoOpKind::Read, poll, |_| {
            (buf.filled().len() - before) as u64
        })
    }
}

#[cfg(feature = "tokio")]
impl<T: AsyncWrite> AsyncWrite for InstrumentedIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.project();
        begin_async_op(this.write_op);
        let poll = this.inner.poll_write(cx, buf);
        finish_async_op(this.write_op, *this.id, IoOpKind::Write, poll, |n| {
            *n as u64
        })
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.project();
        begin_async_op(this.write_op);
        let poll = this.inner.poll_write_vectored(cx, bufs);
        finish_async_op(this.write_op, *this.id, IoOpKind::Write, poll, |n| {
            *n as u64
        })
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.project();
        begin_async_op(this.flush_op);
        let poll = this.inner.poll_flush(cx);
        finish_async_op(this.flush_op, *this.id, IoOpKind::Flush, poll, |_| 0)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.project();
        begin_async_op(this.shutdown_op);
        let poll = this.inner.poll_shutdown(cx);
        finish_async_op(this.shutdown_op, *this.id, IoOpKind::Shutdown, poll, |_| 0)
    }
}
