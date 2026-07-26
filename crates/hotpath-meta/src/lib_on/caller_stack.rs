//! Thread-local stack of instrumented-function names used to attribute SQL
//! queries and HTTP requests to the innermost measured function (the "Source"
//! column). The meta crate carries no SQL or HTTP front-end, so these compile
//! to no-ops; the call sites in the measurement guards stay in place so the
//! guard code mirrors the main crate.

#[inline]
pub(crate) fn push_caller(_name: &'static str) {}

#[inline]
pub(crate) fn pop_caller() {}

#[inline]
#[allow(dead_code)]
pub(crate) fn current_caller() -> Option<&'static str> {
    None
}
