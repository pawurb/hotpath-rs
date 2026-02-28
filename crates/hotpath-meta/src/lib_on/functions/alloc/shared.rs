#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocMetric {
    Bytes,
    Count,
}

pub(crate) fn alloc_metric() -> AllocMetric {
    match std::env::var("HOTPATH_META_ALLOC_METRIC") {
        Ok(v) => match v.to_lowercase().as_str() {
            "bytes" => AllocMetric::Bytes,
            "count" => AllocMetric::Count,
            other => panic!(
                "Invalid HOTPATH_META_ALLOC_METRIC value: '{}'. Expected 'bytes' or 'count'.",
                other
            ),
        },
        Err(_) => AllocMetric::Bytes,
    }
}

#[inline]
pub(crate) fn is_alloc_self_enabled() -> bool {
    std::env::var("HOTPATH_META_ALLOC_SELF")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}
