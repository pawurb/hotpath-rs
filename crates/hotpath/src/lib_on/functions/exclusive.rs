//! Exclusive (self) time mode for function timing, enabled with
//! `HOTPATH_TIME_EXCLUSIVE`: a function reports its own time without the time
//! spent in nested measured functions.
//!
//! Every guard already knows its inclusive duration, so no extra clock reads
//! are needed: exclusive = inclusive - time covered by direct children. The
//! per-thread state below belongs to the innermost open frame, and each frame
//! keeps its parent's state while it is open, so the frame stack lives in the
//! guards themselves and has no depth limit.
//!
//! Sync children run one after another, so their durations are summed. An
//! async function uses the same definition on wall-clock time (time suspended
//! at an `.await` is its own unless a measured child was running), but async
//! children can run concurrently and overlap. They are therefore covered as a
//! union: the stretch from the first async child in flight to the last one
//! returning counts once, in whatever frame they run (the polls of an async
//! parent, or a sync function driving a runtime). Every poll of a measured
//! async body is a frame, and the state moves through the call's
//! `AsyncCallBridge` between polls because consecutive polls can run on
//! different threads.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use crate::instant::Instant;
use crate::lib_on::functions::async_bridge::AsyncCallBridge;

pub(crate) static TIME_EXCLUSIVE: LazyLock<bool> =
    LazyLock::new(|| crate::shared::env_flag("HOTPATH_TIME_EXCLUSIVE"));

pub(crate) const DESCRIPTION: &str =
    "Exclusive execution time of functions (excluding nested measured calls).";

/// Identity of a frame, for an async child to find the frame it started in.
/// Unique in the process: every thread hands out ids from its own block, so
/// the only atomic update happens once per block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameId(u64);

impl FrameId {
    /// Outside of any frame, and async calls in inclusive mode.
    pub(crate) const NONE: Self = Self(0);

    const BLOCK_SIZE: u64 = 1 << 32;

    /// `None` during thread teardown.
    #[inline]
    pub(crate) fn next() -> Option<Self> {
        FRAMES.try_with(Self::next_on).ok()
    }

    #[inline]
    fn next_on(frames: &ThreadFrames) -> Self {
        static NEXT_BLOCK: AtomicU64 = AtomicU64::new(1);

        let mut id = frames.next_id.get();
        // The first id of a block is never handed out, which also keeps
        // `NONE` free.
        if id & (Self::BLOCK_SIZE - 1) == 0 {
            id = NEXT_BLOCK.fetch_add(1, Ordering::Relaxed) * Self::BLOCK_SIZE + 1;
        }
        frames.next_id.set(id.wrapping_add(1));
        Self(id)
    }
}

/// The part of a frame's time that is covered by its children.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChildCover {
    /// Covered by the children that already returned.
    pub(crate) returned_ns: u64,
    /// Async children that started and have not returned yet.
    pub(crate) in_flight: u32,
    /// When `in_flight` last left zero, in nanoseconds since the profiling
    /// start.
    pub(crate) in_flight_since_ns: u64,
}

impl ChildCover {
    pub(crate) const EMPTY: Self = Self {
        returned_ns: 0,
        in_flight: 0,
        in_flight_since_ns: 0,
    };

    /// Async children in flight already cover a sync child.
    #[inline]
    fn sync_child_returned(&mut self, inclusive_ns: u64) {
        if self.in_flight == 0 {
            self.returned_ns = self.returned_ns.saturating_add(inclusive_ns);
        }
    }

    #[inline]
    fn async_child_started(&mut self, start_ns: u64) {
        if self.in_flight == 0 {
            self.in_flight_since_ns = start_ns;
        }
        self.in_flight += 1;
    }

    #[inline]
    fn async_child_returned(&mut self, end_ns: u64) {
        self.in_flight -= 1;
        if self.in_flight == 0 {
            let stretch_ns = end_ns.saturating_sub(self.in_flight_since_ns);
            self.returned_ns = self.returned_ns.saturating_add(stretch_ns);
        }
    }

    /// Covered time of a frame whose call ends at `end_ns`. Async children
    /// still in flight then (the call was cancelled) cover it to the end.
    #[inline]
    fn total_ns(&self, end_ns: impl FnOnce() -> u64) -> u64 {
        if self.in_flight == 0 {
            return self.returned_ns;
        }
        self.returned_ns
            .saturating_add(end_ns().saturating_sub(self.in_flight_since_ns))
    }
}

#[derive(Clone, Copy)]
struct FrameState {
    id: FrameId,
    children: ChildCover,
}

struct ThreadFrames {
    /// State of the innermost open frame on this thread.
    innermost: Cell<FrameState>,
    next_id: Cell<u64>,
}

thread_local! {
    static FRAMES: ThreadFrames = const {
        ThreadFrames {
            innermost: Cell::new(FrameState {
                id: FrameId::NONE,
                children: ChildCover::EMPTY,
            }),
            next_id: Cell::new(0),
        }
    };
}

/// An open frame. Holds the enclosing frame's state, which the thread-local
/// stops tracking while this frame is the innermost one.
pub(crate) struct ExclusiveFrame {
    parent: FrameState,
}

impl ExclusiveFrame {
    /// `None` when exclusive mode is off. `try_with` because guards can be
    /// built during thread teardown.
    #[inline]
    pub(crate) fn enter() -> Option<Self> {
        if !*TIME_EXCLUSIVE {
            return None;
        }
        FRAMES
            .try_with(|frames| Self {
                parent: frames.innermost.replace(FrameState {
                    id: FrameId::next_on(frames),
                    children: ChildCover::EMPTY,
                }),
            })
            .ok()
    }

    /// Reopens the frame of an async call for one poll of its body.
    #[inline]
    fn enter_poll(bridge: &AsyncCallBridge) -> Option<Self> {
        let state = FrameState {
            id: bridge.frame_id(),
            children: bridge.children(),
        };
        FRAMES
            .try_with(|frames| Self {
                parent: frames.innermost.replace(state),
            })
            .ok()
    }

    /// Closes the frame on the thread that opened it: hands `inclusive_ns` up
    /// to the enclosing frame and returns this frame's exclusive time.
    #[inline]
    fn exit(mut self, inclusive_ns: u64, end: Instant) -> Option<u64> {
        self.parent.children.sync_child_returned(inclusive_ns);
        let own = FRAMES
            .try_with(|frames| frames.innermost.replace(self.parent))
            .ok()?;
        let covered_ns = own
            .children
            .total_ns(|| crate::lib_on::elapsed_since_start_ns(end));
        Some(inclusive_ns.saturating_sub(covered_ns))
    }

    /// Closes a poll frame and returns what its children cover so far.
    /// Nothing is handed up: the async call reports to its parent when its
    /// guard is built and dropped.
    #[inline]
    fn exit_poll(&self) -> Option<ChildCover> {
        FRAMES
            .try_with(|frames| frames.innermost.replace(self.parent).children)
            .ok()
    }
}

/// Duration a sync guard reports. In exclusive mode a guard dropped on another
/// thread reports no duration: its frame belongs to the thread that opened
/// it, which only loses the children sum of the enclosing frame.
#[inline]
pub(crate) fn sync_duration_ns(
    frame: Option<ExclusiveFrame>,
    start: Option<Instant>,
    end: Instant,
    cross_thread: bool,
) -> Option<u64> {
    let inclusive_ns = start.map(|start| end.duration_since(start).as_nanos() as u64);
    if !*TIME_EXCLUSIVE {
        return inclusive_ns;
    }
    if cross_thread {
        return None;
    }
    let self_ns = frame?.exit(inclusive_ns.unwrap_or(0), end)?;
    inclusive_ns.map(|_| self_ns)
}

/// The exclusive-time side of an async guard. The guard is built and dropped
/// inside its parent's polls (on whichever threads run them), so it never
/// opens a frame of its own: it tells the innermost frame when it starts and
/// ends, and subtracts what its own polls collected in the bridge.
pub(crate) struct AsyncCall {
    bridge: Option<Arc<AsyncCallBridge>>,
    /// The frame the call started in. `None` until `started`, and for calls
    /// that are not timed.
    parent: Option<FrameId>,
}

impl AsyncCall {
    #[inline]
    pub(crate) fn new(bridge: Option<Arc<AsyncCallBridge>>) -> Self {
        Self {
            bridge,
            parent: None,
        }
    }

    #[inline]
    pub(crate) fn bridge(&self) -> Option<&AsyncCallBridge> {
        self.bridge.as_deref()
    }

    /// Registers the call with the innermost frame. `start` is the guard's
    /// start time, `None` when the call is not timed.
    #[inline]
    pub(crate) fn started(&mut self, start: Option<Instant>) {
        let Some(start) = start else { return };
        if !*TIME_EXCLUSIVE {
            return;
        }
        let start_ns = crate::lib_on::elapsed_since_start_ns(start);
        self.parent = FRAMES
            .try_with(|frames| {
                let mut state = frames.innermost.get();
                state.children.async_child_started(start_ns);
                frames.innermost.set(state);
                state.id
            })
            .ok();
    }

    /// Duration the guard reports.
    #[inline]
    pub(crate) fn duration_ns(&self, start: Option<Instant>, end: Instant) -> Option<u64> {
        let inclusive_ns = end.duration_since(start?).as_nanos() as u64;
        if !*TIME_EXCLUSIVE {
            return Some(inclusive_ns);
        }
        let end_ns = crate::lib_on::elapsed_since_start_ns(end);
        self.ended(end_ns);
        let covered_ns = self
            .bridge()
            .map_or(0, |bridge| bridge.children().total_ns(|| end_ns));
        Some(inclusive_ns.saturating_sub(covered_ns))
    }

    /// Reports the end of the call to the frame it started in. A guard
    /// dropped anywhere else (its future was moved out of the parent) reports
    /// nothing, rather than corrupting an unrelated frame.
    #[inline]
    fn ended(&self, end_ns: u64) {
        let Some(parent) = self.parent else { return };
        let _ = FRAMES.try_with(|frames| {
            let mut state = frames.innermost.get();
            if state.id != parent || state.children.in_flight == 0 {
                return;
            }
            state.children.async_child_returned(end_ns);
            frames.innermost.set(state);
        });
    }
}

/// Frame around one poll of a measured async body: collects the time covered
/// by children during the poll. Closes on drop, so a panicking poll still
/// restores the enclosing frame.
pub(crate) struct PollFrame<'a> {
    frame: ExclusiveFrame,
    bridge: &'a AsyncCallBridge,
}

impl<'a> PollFrame<'a> {
    /// `None` for futures that are not measured function bodies (no bridge)
    /// and when exclusive mode is off.
    #[inline]
    pub(crate) fn enter(bridge: Option<&'a AsyncCallBridge>) -> Option<Self> {
        let bridge = bridge?;
        if !*TIME_EXCLUSIVE {
            return None;
        }
        let frame = ExclusiveFrame::enter_poll(bridge)?;
        Some(Self { frame, bridge })
    }
}

impl Drop for PollFrame<'_> {
    #[inline]
    fn drop(&mut self) {
        if let Some(children) = self.frame.exit_poll() {
            self.bridge.set_children(children);
        }
    }
}
