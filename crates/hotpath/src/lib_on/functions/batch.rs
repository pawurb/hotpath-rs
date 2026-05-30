use crossbeam_channel::Sender;

pub(crate) const BATCH_SIZE: usize = 1;
pub(crate) const FLUSH_INTERVAL_MS: u64 = 50;
pub(crate) const FLUSH_INTERVAL_NS: u64 = FLUSH_INTERVAL_MS * 1_000_000;

pub(crate) trait BatchedMeasurement: Sized {
    fn elapsed_since_start_ns(&self) -> u64;
    fn fetch_sender() -> Option<Sender<Vec<Self>>>;
}

pub(crate) struct MeasurementBatch<M: BatchedMeasurement> {
    measurements: Vec<M>,
    last_flush_elapsed_ns: u64,
    sender: Option<Sender<Vec<M>>>,
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
        self.measurements.push(measurement);

        let should_flush = self.measurements.len() >= BATCH_SIZE
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
        let _ = sender.send(batch);
    }
}

impl<M: BatchedMeasurement> Drop for MeasurementBatch<M> {
    fn drop(&mut self) {
        self.flush();
    }
}
