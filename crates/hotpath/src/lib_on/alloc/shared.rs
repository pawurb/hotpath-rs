#[inline]
pub(crate) fn is_alloc_self_enabled() -> bool {
    std::env::var("HOTPATH_ALLOC_SELF")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}
