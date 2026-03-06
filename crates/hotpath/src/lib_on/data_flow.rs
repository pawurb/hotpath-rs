//! Unified data flow module - provides shared counter and types for channels, streams, and futures.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::channels::{get_sorted_channel_entries, ChannelEntry, START_TIME};
use crate::futures::{get_sorted_future_stats, FutureEntry};
use crate::json::{
    DataFlowType, JsonDataFlowEntry, JsonDataFlowList, JsonFutureEntry, JsonStreamEntry,
};
use crate::streams::{get_sorted_stream_stats, StreamStats};

pub(crate) static DATA_FLOW_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

pub(crate) const WORKER_BATCH_SIZE: usize = 100;
pub(crate) const WORKER_FLUSH_INTERVAL_MS: u64 = 50;
pub(crate) use crate::lib_on::hotpath_guard::WORKER_SHUTDOWN_DRAIN_LIMIT;

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn next_data_flow_id() -> u32 {
    DATA_FLOW_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

impl From<&ChannelEntry> for JsonDataFlowEntry {
    fn from(stats: &ChannelEntry) -> Self {
        JsonDataFlowEntry {
            id: stats.id,
            data_flow_type: DataFlowType::Channel,
            source: stats.source.to_string(),
            label: crate::channels::resolve_label(
                stats.source,
                stats.label.as_deref(),
                Some(stats.iter),
            ),
            has_custom_label: stats.label.is_some(),
            state: stats.state.as_str().to_string(),
            subtype: Some(stats.channel_type.to_string()),
            primary_count: stats.sent_count,
            secondary_count: Some(stats.received_count),
            type_name: Some(stats.type_name.to_string()),
            type_size: Some(stats.type_size),
            iter: Some(stats.iter),
        }
    }
}

impl From<&StreamStats> for JsonDataFlowEntry {
    fn from(stats: &StreamStats) -> Self {
        let entry: JsonStreamEntry = stats.into();
        JsonDataFlowEntry {
            id: entry.id,
            data_flow_type: DataFlowType::Stream,
            source: entry.source,
            label: entry.label,
            has_custom_label: entry.has_custom_label,
            state: entry.state,
            subtype: None,
            primary_count: entry.items_yielded,
            secondary_count: None,
            type_name: Some(entry.type_name),
            type_size: Some(entry.type_size),
            iter: Some(entry.iter),
        }
    }
}

impl From<&FutureEntry> for JsonDataFlowEntry {
    fn from(stats: &FutureEntry) -> Self {
        let entry: JsonFutureEntry = stats.into();
        JsonDataFlowEntry {
            id: entry.id,
            data_flow_type: DataFlowType::Future,
            source: entry.source,
            label: entry.label,
            has_custom_label: entry.has_custom_label,
            state: "active".to_string(),
            subtype: None,
            primary_count: entry.call_count,
            secondary_count: None,
            type_name: None,
            type_size: None,
            iter: None,
        }
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_data_flow_json() -> JsonDataFlowList {
    let mut entries: Vec<JsonDataFlowEntry> = Vec::new();

    for stats in get_sorted_channel_entries() {
        entries.push(JsonDataFlowEntry::from(&stats));
    }

    for stats in get_sorted_stream_stats() {
        entries.push(JsonDataFlowEntry::from(&stats));
    }

    for stats in get_sorted_future_stats() {
        entries.push(JsonDataFlowEntry::from(&stats));
    }

    entries.sort_by(|a, b| {
        b.has_custom_label
            .cmp(&a.has_custom_label)
            .then_with(|| {
                a.data_flow_type
                    .sort_order()
                    .cmp(&b.data_flow_type.sort_order())
            })
            .then_with(|| a.id.cmp(&b.id))
    });

    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    JsonDataFlowList {
        current_elapsed_ns,
        entries,
    }
}
