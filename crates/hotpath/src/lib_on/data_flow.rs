//! Unified data flow module - provides shared counter and types for channels, streams, and futures.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::channels::{get_sorted_channel_entries, ChannelEntry, ChannelType, START_TIME};
use crate::futures::{get_sorted_future_stats, FutureEntry};
use crate::json::{
    DataFlowType, JsonChannelEntry, JsonDataFlowEntry, JsonDataFlowList, JsonFutureEntry,
    JsonStreamEntry,
};
use crate::streams::{get_sorted_stream_stats, StreamStats};

pub static DATA_FLOW_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub fn next_data_flow_id() -> u64 {
    DATA_FLOW_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn format_queue(channel_type: &ChannelType, queued: u64) -> Option<String> {
    match channel_type {
        ChannelType::Unbounded => None,
        ChannelType::Oneshot => Some(format!("{}/1", queued)),
        ChannelType::Bounded(capacity) => Some(format!("{}/{}", queued, capacity)),
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn format_max_queue(channel_type: &ChannelType, max_queued: u64) -> Option<String> {
    match channel_type {
        ChannelType::Unbounded => None,
        ChannelType::Oneshot => Some(format!("{}/1", max_queued)),
        ChannelType::Bounded(capacity) => Some(format!("{}/{}", max_queued, capacity)),
    }
}

impl From<&ChannelEntry> for JsonDataFlowEntry {
    fn from(stats: &ChannelEntry) -> Self {
        let entry: JsonChannelEntry = stats.into();
        let queue = format_queue(&stats.channel_type, entry.queued);
        let max_queue = format_max_queue(&stats.channel_type, entry.max_queued);
        let queue_mem = if queue.is_some() {
            Some(entry.queued_bytes)
        } else {
            None
        };
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
            queue,
            queue_mem,
            max_queue,
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
            queue: None,
            queue_mem: None,
            max_queue: None,
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
            queue: None,
            queue_mem: None,
            max_queue: None,
            type_name: None,
            type_size: None,
            iter: None,
        }
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub fn get_data_flow_json() -> JsonDataFlowList {
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
