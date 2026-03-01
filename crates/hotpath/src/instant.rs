#[cfg(target_os = "linux")]
pub(crate) type Instant = quanta::Instant;

#[cfg(not(target_os = "linux"))]
pub(crate) type Instant = std::time::Instant;
