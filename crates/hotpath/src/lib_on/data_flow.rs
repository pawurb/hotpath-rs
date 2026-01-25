//! Unified data flow module - provides shared counter and types for channels, streams, and futures.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::channels::{get_sorted_channel_stats, ChannelStats, START_TIME};
use crate::futures::{get_sorted_future_stats, FutureStats};
use crate::json::{
    DataFlowType, JsonChannelEntry, JsonDataFlowEntry, JsonDataFlowList, JsonFutureEntry,
    JsonStreamEntry,
};
use crate::streams::{get_sorted_stream_stats, StreamStats};

pub static DATA_FLOW_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_data_flow_id() -> u64 {
    DATA_FLOW_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

impl From<&ChannelStats> for JsonDataFlowEntry {
    fn from(stats: &ChannelStats) -> Self {
        let entry: JsonChannelEntry = stats.into();
        JsonDataFlowEntry {
            id: entry.id,
            data_flow_type: DataFlowType::Channel,
            source: entry.source,
            label: entry.label,
            has_custom_label: entry.has_custom_label,
            state: entry.state,
            subtype: Some(entry.channel_type),
            primary_count: entry.sent_count,
            secondary_count: Some(entry.received_count),
            type_name: Some(entry.type_name),
            type_size: Some(entry.type_size),
            iter: Some(entry.iter),
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

impl From<&FutureStats> for JsonDataFlowEntry {
    fn from(stats: &FutureStats) -> Self {
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

pub fn get_data_flow_json() -> JsonDataFlowList {
    let mut entries: Vec<JsonDataFlowEntry> = Vec::new();

    for stats in get_sorted_channel_stats() {
        entries.push(JsonDataFlowEntry::from(&stats));
    }

    for stats in get_sorted_stream_stats() {
        entries.push(JsonDataFlowEntry::from(&stats));
    }

    for stats in get_sorted_future_stats() {
        entries.push(JsonDataFlowEntry::from(&stats));
    }

    entries.sort_by(|a, b| a.id.cmp(&b.id));

    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    JsonDataFlowList {
        current_elapsed_ns,
        entries,
    }
}
