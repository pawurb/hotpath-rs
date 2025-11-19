pub mod profile_pr;

#[cfg(all(feature = "tui", not(feature = "hotpath-off")))]
pub mod console;
