use std::sync::{Arc, Mutex, Weak};

pub(crate) const BATCH_SIZE: usize = 64;
pub(crate) const FLUSH_INTERVAL_MS: u64 = 50;
pub(crate) const FLUSH_INTERVAL_NS: u64 = FLUSH_INTERVAL_MS * 1_000_000;

pub(crate) trait BatchedMeasurement: Sized + Send {
    /// Sender the batch flushes into: a plain `crossbeam_channel::Sender`, or a
    /// `wrap = true` sender under `hotpath-meta`.
    type Tx: Clone + Send + 'static;

    fn elapsed_since_start_ns(&self) -> u64;
    fn fetch_sender() -> Option<Self::Tx>;

    /// Wraps a flushed batch into the worker's message type and sends it.
    fn send_batch(tx: &Self::Tx, batch: Vec<Self>);

    /// Lifecycle events that must reach the worker before the data events they
    /// gate (e.g. `Created`) force an immediate flush so per-thread batching
    /// can't deliver a data event ahead of the entry that establishes its slot.
    fn is_flush_boundary(&self) -> bool {
        false
    }
}

pub(crate) struct MeasurementBatch<M: BatchedMeasurement> {
    measurements: Vec<M>,
    last_flush_elapsed_ns: u64,
    sender: Option<M::Tx>,
}

impl<M: BatchedMeasurement> MeasurementBatch<M> {
    pub(crate) fn new() -> Self {
        Self {
            measurements: Vec::with_capacity(BATCH_SIZE),
            last_flush_elapsed_ns: 0,
            sender: None,
        }
    }

    pub(crate) fn add(&mut self, measurement: M) {
        if self.sender.is_none() {
            self.sender = M::fetch_sender();
        }

        if self.sender.is_none() {
            return;
        }

        let elapsed_since_start_ns = measurement.elapsed_since_start_ns();
        let is_boundary = measurement.is_flush_boundary();
        self.measurements.push(measurement);

        let should_flush = is_boundary
            || self.measurements.len() >= BATCH_SIZE
            || elapsed_since_start_ns.saturating_sub(self.last_flush_elapsed_ns)
                >= FLUSH_INTERVAL_NS;

        if should_flush {
            self.flush();
        }
    }

    pub(crate) fn flush(&mut self) {
        if self.measurements.is_empty() {
            return;
        }

        let sender = self.sender.as_ref().expect("Sender must exist");
        if let Some(last) = self.measurements.last() {
            self.last_flush_elapsed_ns = last.elapsed_since_start_ns();
        }
        let batch = std::mem::replace(&mut self.measurements, Vec::with_capacity(BATCH_SIZE));
        M::send_batch(sender, batch);
    }
}

impl<M: BatchedMeasurement> Drop for MeasurementBatch<M> {
    fn drop(&mut self) {
        self.flush();
    }
}

/// Registry of every live per-thread [`MeasurementBatch`] for a given event type.
///
/// Producing threads keep their batch in thread-local storage for a lock-light
/// hot path, but those batches are unreachable from other threads. On shutdown
/// the producing threads may still be alive (e.g. parked async runtime workers)
/// with buffered events that have not reached the worker. The registry holds a
/// `Weak` to each batch so [`BatchRegistry::flush_all`] can drain them all from
/// the shutting-down thread before the worker stops.
pub(crate) struct BatchRegistry<M: BatchedMeasurement> {
    batches: Mutex<Vec<Weak<Mutex<MeasurementBatch<M>>>>>,
}

impl<M: BatchedMeasurement> BatchRegistry<M> {
    pub(crate) const fn new() -> Self {
        Self {
            batches: Mutex::new(Vec::new()),
        }
    }

    fn register(&self, batch: &Arc<Mutex<MeasurementBatch<M>>>) {
        if let Ok(mut batches) = self.batches.lock() {
            batches.push(Arc::downgrade(batch));
        }
    }

    pub(crate) fn flush_all(&self) {
        if let Ok(mut batches) = self.batches.lock() {
            batches.retain(|weak| match weak.upgrade() {
                Some(batch) => {
                    if let Ok(mut batch) = batch.lock() {
                        batch.flush();
                    }
                    true
                }
                None => false,
            });
        }
    }
}

/// Creates a fresh per-thread batch and registers it for shutdown draining.
pub(crate) fn register_thread_batch<M: BatchedMeasurement>(
    registry: &'static BatchRegistry<M>,
) -> Arc<Mutex<MeasurementBatch<M>>> {
    let batch = Arc::new(Mutex::new(MeasurementBatch::new()));
    registry.register(&batch);
    batch
}
