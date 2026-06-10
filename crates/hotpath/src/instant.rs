#[cfg(target_os = "linux")]
pub type Instant = quanta::Instant;

#[cfg(target_os = "macos")]
pub type Instant = mach_instant::Instant;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub type Instant = std::time::Instant;

/// Thin wrapper over `mach_absolute_time`. On macOS `std::time::Instant`
/// goes through `clock_gettime(CLOCK_UPTIME_RAW)` (~25ns per call), while
/// reading the counter directly costs ~7ns with the same resolution.
/// Tick-to-nanosecond conversion uses the exact `mach_timebase_info` ratio.
#[cfg(target_os = "macos")]
mod mach_instant {
    use std::sync::OnceLock;
    use std::time::Duration;

    #[repr(C)]
    struct MachTimebaseInfo {
        numer: u32,
        denom: u32,
    }

    extern "C" {
        fn mach_absolute_time() -> u64;
        fn mach_timebase_info(info: *mut MachTimebaseInfo) -> libc::c_int;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Instant(u64);

    #[inline]
    fn timebase() -> (u64, u64) {
        static TIMEBASE: OnceLock<(u64, u64)> = OnceLock::new();
        *TIMEBASE.get_or_init(|| {
            let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
            let ret = unsafe { mach_timebase_info(&mut info) };
            assert_eq!(ret, libc::KERN_SUCCESS, "mach_timebase_info failed");
            (u64::from(info.numer), u64::from(info.denom))
        })
    }

    #[inline]
    fn ticks_to_ns(ticks: u64) -> u64 {
        let (numer, denom) = timebase();
        if numer == denom {
            return ticks;
        }
        match ticks.checked_mul(numer) {
            Some(scaled) => scaled / denom,
            None => ((ticks as u128 * numer as u128) / denom as u128) as u64,
        }
    }

    impl Instant {
        #[inline]
        pub fn now() -> Self {
            Self(unsafe { mach_absolute_time() })
        }

        #[inline]
        pub fn duration_since(&self, earlier: Instant) -> Duration {
            Duration::from_nanos(ticks_to_ns(self.0.saturating_sub(earlier.0)))
        }

        #[inline]
        pub fn elapsed(&self) -> Duration {
            Instant::now().duration_since(*self)
        }
    }
}
