//! Time-sampling support: measure durations for only a fraction of calls while
//! keeping every event flowing (counts, states, queue sizes stay exact).
//!
//! Rates are fractions in `[0.0, 1.0]`: `0.1` times 1 in 10 calls, `0.0` is
//! count-only mode (no durations at all), `1.0` or unset measures everything.
//! Resolution happens once at guard build; events emitted before the guard
//! exists are measured at 100%.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sampler {
    /// Count-only mode (rate 0): never measure durations.
    Never,
    /// Measure 1 in `k` calls (`k >= 2`).
    OneIn(u64),
}

impl Sampler {
    /// `None` means "measure everything" (rate `1.0`, or a rate that rounds to
    /// keep-every-call). Rates are converted to an integer keep-rate
    /// `k = round(1 / rate)`, so e.g. `0.3` becomes exactly 1-in-3.
    pub(crate) fn from_rate(rate: f64) -> Option<Self> {
        if !rate.is_finite() || !(0.0..=1.0).contains(&rate) {
            return None;
        }
        if rate == 0.0 {
            return Some(Sampler::Never);
        }
        let k = (1.0 / rate).round() as u64;
        if k <= 1 {
            None
        } else {
            Some(Sampler::OneIn(k))
        }
    }

    /// Effective fraction of calls timed (0.0 in count-only mode).
    pub(crate) fn effective_rate(&self) -> f64 {
        match self {
            Sampler::Never => 0.0,
            Sampler::OneIn(k) => 1.0 / *k as f64,
        }
    }

    /// Deterministic decision keyed on a monotonic id (wrap-channel `msg_id`).
    /// Id 0 is always sampled, so every channel measures its first message.
    #[inline]
    pub(crate) fn sample_id(&self, id: u64) -> bool {
        match self {
            Sampler::Never => false,
            Sampler::OneIn(k) => id.is_multiple_of(*k),
        }
    }

    #[inline]
    fn sample_counter(&self, counter: &Cell<u64>) -> bool {
        let n = counter.get();
        counter.set(n.wrapping_add(1));
        self.sample_id(n)
    }
}

/// Per-resource sampling slot, set once at guard build. Unset means measure
/// every call, so pre-guard events keep exact timings.
pub(crate) struct ResourceSampling {
    slot: OnceLock<Option<Sampler>>,
}

impl ResourceSampling {
    const fn new() -> Self {
        Self {
            slot: OnceLock::new(),
        }
    }

    fn set(&self, sampler: Option<Sampler>) {
        let _ = self.slot.set(sampler);
    }

    #[inline]
    pub(crate) fn sampler(&self) -> Option<Sampler> {
        self.slot.get().copied().flatten()
    }
}

pub(crate) static FUNCTIONS_SAMPLING: ResourceSampling = ResourceSampling::new();
pub(crate) static MUTEXES_SAMPLING: ResourceSampling = ResourceSampling::new();
pub(crate) static RW_LOCKS_SAMPLING: ResourceSampling = ResourceSampling::new();
pub(crate) static FUTURES_SAMPLING: ResourceSampling = ResourceSampling::new();
pub(crate) static CHANNELS_SAMPLING: ResourceSampling = ResourceSampling::new();
pub(crate) static IO_SAMPLING: ResourceSampling = ResourceSampling::new();

thread_local! {
    static FUNCTIONS_COUNTER: Cell<u64> = const { Cell::new(0) };
    static MUTEXES_COUNTER: Cell<u64> = const { Cell::new(0) };
    static RW_LOCKS_COUNTER: Cell<u64> = const { Cell::new(0) };
    static FUTURES_COUNTER: Cell<u64> = const { Cell::new(0) };
    // One counter per I/O operation kind (read/write/flush/shutdown): a shared
    // counter would let deterministic 1-in-k sampling lock onto periodic
    // workloads (e.g. an alternating write/read loop at rate 0.5 timing only
    // writes) and bias per-kind stats.
    static IO_COUNTERS: [Cell<u64>; 4] =
        const { [Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0)] };
}

/// Thread-local counter decision: deterministic per thread, first call on a
/// thread is always sampled. `try_with` because guards can drop during thread
/// teardown when the thread-local is already destroyed.
#[inline]
fn should_time(
    sampling: &ResourceSampling,
    counter: &'static std::thread::LocalKey<Cell<u64>>,
) -> bool {
    match sampling.sampler() {
        None => true,
        // Count-only mode never times; skip the counter access entirely.
        Some(Sampler::Never) => false,
        Some(sampler) => counter
            .try_with(|c| sampler.sample_counter(c))
            .unwrap_or(false),
    }
}

/// Rolls back the last decision after a failed try-acquisition, so the rate
/// applies to acquisitions rather than attempts. Only `OneIn` consumes a
/// counter tick in `should_time`, so only then is there anything to undo.
#[inline]
fn untime(sampling: &ResourceSampling, counter: &'static std::thread::LocalKey<Cell<u64>>) {
    if matches!(sampling.sampler(), Some(Sampler::OneIn(_))) {
        let _ = counter.try_with(|c| c.set(c.get().wrapping_sub(1)));
    }
}

#[inline]
pub(crate) fn functions_should_time() -> bool {
    should_time(&FUNCTIONS_SAMPLING, &FUNCTIONS_COUNTER)
}

#[inline]
pub(crate) fn mutexes_should_time() -> bool {
    should_time(&MUTEXES_SAMPLING, &MUTEXES_COUNTER)
}

#[inline]
pub(crate) fn mutexes_untime() {
    untime(&MUTEXES_SAMPLING, &MUTEXES_COUNTER)
}

#[inline]
pub(crate) fn rw_locks_should_time() -> bool {
    should_time(&RW_LOCKS_SAMPLING, &RW_LOCKS_COUNTER)
}

#[inline]
pub(crate) fn rw_locks_untime() {
    untime(&RW_LOCKS_SAMPLING, &RW_LOCKS_COUNTER)
}

#[inline]
pub(crate) fn futures_should_time() -> bool {
    should_time(&FUTURES_SAMPLING, &FUTURES_COUNTER)
}

#[inline]
pub(crate) fn io_should_time(kind_idx: usize) -> bool {
    match IO_SAMPLING.sampler() {
        None => true,
        // Count-only mode never times; skip the counter access entirely.
        Some(Sampler::Never) => false,
        Some(sampler) => IO_COUNTERS
            .try_with(|counters| sampler.sample_counter(&counters[kind_idx]))
            .unwrap_or(false),
    }
}

#[inline]
pub(crate) fn io_untime(kind_idx: usize) {
    if matches!(IO_SAMPLING.sampler(), Some(Sampler::OneIn(_))) {
        let _ = IO_COUNTERS.try_with(|counters| {
            let counter = &counters[kind_idx];
            counter.set(counter.get().wrapping_sub(1));
        });
    }
}

/// Wrap-channel decision, keyed on the per-channel monotonic `msg_id` so both
/// endpoints agree without extra state and tests are deterministic across
/// sender threads.
#[inline]
pub(crate) fn channels_should_time(msg_id: u64) -> bool {
    match CHANNELS_SAMPLING.sampler() {
        None => true,
        Some(sampler) => sampler.sample_id(msg_id),
    }
}

/// Builder-provided rates, resolved against env vars at guard build.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TimeSamplingConfig {
    pub(crate) global: Option<f64>,
    pub(crate) functions: Option<f64>,
    pub(crate) mutexes: Option<f64>,
    pub(crate) rw_locks: Option<f64>,
    pub(crate) futures: Option<f64>,
    pub(crate) channels: Option<f64>,
    pub(crate) io: Option<f64>,
}

/// Invalid values (negative, > 1.0, NaN, unparseable) are ignored, falling
/// through to the next precedence level.
fn parse_rate_env(name: &str) -> Option<f64> {
    let rate: f64 = std::env::var(name).ok()?.parse().ok()?;
    (rate.is_finite() && (0.0..=1.0).contains(&rate)).then_some(rate)
}

/// Resolves and locks in per-resource samplers. Precedence:
/// per-resource env > global env > per-resource builder > global builder.
pub(crate) fn init_time_sampling_rate(config: &TimeSamplingConfig) {
    let global_env = parse_rate_env("HOTPATH_TIME_SAMPLING_RATE");
    let resolve = |env_name: &str, builder_rate: Option<f64>| {
        parse_rate_env(env_name)
            .or(global_env)
            .or(builder_rate)
            .or(config.global)
            .and_then(Sampler::from_rate)
    };
    FUNCTIONS_SAMPLING.set(resolve(
        "HOTPATH_FUNCTIONS_TIME_SAMPLING_RATE",
        config.functions,
    ));
    MUTEXES_SAMPLING.set(resolve(
        "HOTPATH_MUTEXES_TIME_SAMPLING_RATE",
        config.mutexes,
    ));
    RW_LOCKS_SAMPLING.set(resolve(
        "HOTPATH_RW_LOCKS_TIME_SAMPLING_RATE",
        config.rw_locks,
    ));
    FUTURES_SAMPLING.set(resolve(
        "HOTPATH_FUTURES_TIME_SAMPLING_RATE",
        config.futures,
    ));
    CHANNELS_SAMPLING.set(resolve(
        "HOTPATH_CHANNELS_TIME_SAMPLING_RATE",
        config.channels,
    ));
    IO_SAMPLING.set(resolve("HOTPATH_IO_TIME_SAMPLING_RATE", config.io));
}

/// Effective per-resource rates for the report header / JSON, `None` when no
/// sampler is active.
pub(crate) fn active_rates() -> Option<HashMap<String, f64>> {
    let mut rates = HashMap::new();
    for (name, sampling) in [
        ("functions", &FUNCTIONS_SAMPLING),
        ("mutexes", &MUTEXES_SAMPLING),
        ("rw_locks", &RW_LOCKS_SAMPLING),
        ("futures", &FUTURES_SAMPLING),
        ("channels", &CHANNELS_SAMPLING),
        ("io", &IO_SAMPLING),
    ] {
        if let Some(sampler) = sampling.sampler() {
            rates.insert(name.to_string(), sampler.effective_rate());
        }
    }
    (!rates.is_empty()).then_some(rates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_rate_bounds() {
        assert_eq!(Sampler::from_rate(1.0), None);
        assert_eq!(Sampler::from_rate(0.0), Some(Sampler::Never));
        assert_eq!(Sampler::from_rate(0.5), Some(Sampler::OneIn(2)));
        assert_eq!(Sampler::from_rate(0.1), Some(Sampler::OneIn(10)));
        assert_eq!(Sampler::from_rate(0.001), Some(Sampler::OneIn(1000)));
        assert_eq!(Sampler::from_rate(0.3), Some(Sampler::OneIn(3)));
        assert_eq!(Sampler::from_rate(-0.1), None);
        assert_eq!(Sampler::from_rate(1.5), None);
        assert_eq!(Sampler::from_rate(f64::NAN), None);
        // Rounds to k = 1, which keeps every call: sampler disabled.
        assert_eq!(Sampler::from_rate(0.9), None);
    }

    #[test]
    fn sample_id_keeps_one_in_k() {
        let s = Sampler::OneIn(10);
        let kept = (0..100).filter(|&i| s.sample_id(i)).count();
        assert_eq!(kept, 10);
        assert!(s.sample_id(0));
        assert!(!Sampler::Never.sample_id(0));
    }

    #[test]
    fn counter_sampling_is_deterministic() {
        let s = Sampler::OneIn(3);
        let c = Cell::new(0);
        let pattern: Vec<bool> = (0..6).map(|_| s.sample_counter(&c)).collect();
        assert_eq!(pattern, [true, false, false, true, false, false]);
    }
}
