use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::lib_on::functions::exclusive::{ChildCover, FrameId, TIME_EXCLUSIVE};

/// Per-call state of a measured async function, fed by the poll wrapper
/// (`futures::wrapper`) around every poll of the body and read by the async
/// guard when the call ends.
#[derive(Debug)]
pub(crate) struct AsyncCallBridge {
    #[cfg(feature = "hotpath-alloc")]
    bytes_total: AtomicU64,
    #[cfg(feature = "hotpath-alloc")]
    count_total: AtomicU64,
    /// The call's exclusive-time frame (`HOTPATH_TIME_EXCLUSIVE`), kept here
    /// between the polls that reopen it.
    frame_id: FrameId,
    children_returned_ns: AtomicU64,
    children_in_flight: AtomicU32,
    children_in_flight_since_ns: AtomicU64,
}

impl AsyncCallBridge {
    pub(crate) fn new() -> Self {
        let frame_id = if *TIME_EXCLUSIVE {
            FrameId::next().unwrap_or(FrameId::NONE)
        } else {
            FrameId::NONE
        };
        Self {
            #[cfg(feature = "hotpath-alloc")]
            bytes_total: AtomicU64::new(0),
            #[cfg(feature = "hotpath-alloc")]
            count_total: AtomicU64::new(0),
            frame_id,
            children_returned_ns: AtomicU64::new(0),
            children_in_flight: AtomicU32::new(0),
            children_in_flight_since_ns: AtomicU64::new(0),
        }
    }

    #[cfg(feature = "hotpath-alloc")]
    #[inline]
    pub(crate) fn add_alloc(&self, bytes: u64, count: u64) {
        self.bytes_total.fetch_add(bytes, Ordering::Relaxed);
        self.count_total.fetch_add(count, Ordering::Relaxed);
    }

    #[cfg(not(feature = "hotpath-alloc"))]
    #[inline]
    pub(crate) fn add_alloc(&self, _bytes: u64, _count: u64) {}

    #[cfg(feature = "hotpath-alloc")]
    #[inline]
    pub(crate) fn alloc_snapshot(&self) -> (Option<u64>, Option<u64>) {
        (
            Some(self.bytes_total.load(Ordering::Relaxed)),
            Some(self.count_total.load(Ordering::Relaxed)),
        )
    }

    #[inline]
    pub(crate) fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    #[inline]
    pub(crate) fn children(&self) -> ChildCover {
        ChildCover {
            returned_ns: self.children_returned_ns.load(Ordering::Relaxed),
            in_flight: self.children_in_flight.load(Ordering::Relaxed),
            in_flight_since_ns: self.children_in_flight_since_ns.load(Ordering::Relaxed),
        }
    }

    /// Polls of one future never overlap, so plain stores are enough.
    #[inline]
    pub(crate) fn set_children(&self, children: ChildCover) {
        self.children_returned_ns
            .store(children.returned_ns, Ordering::Relaxed);
        self.children_in_flight
            .store(children.in_flight, Ordering::Relaxed);
        self.children_in_flight_since_ns
            .store(children.in_flight_since_ns, Ordering::Relaxed);
    }
}
