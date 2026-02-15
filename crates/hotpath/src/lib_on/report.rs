use std::io::Write;

use prettytable::{Cell, Row, Table};

use crate::channels::{compare_channel_entries, resolve_label, ChannelEntry, CHANNELS_STATE};
use crate::futures::{compare_future_stats, FutureEntry, FUTURES_STATE};
use crate::json::{
    JsonChannelEntry, JsonChannelsList, JsonFutureEntry, JsonFuturesList, JsonStreamEntry,
    JsonStreamsList,
};
use crate::output::format_bytes;
use crate::streams::{compare_stream_stats, StreamStats, STREAMS_STATE};

pub(crate) fn shutdown_channels() -> Vec<ChannelEntry> {
    CHANNELS_STATE
        .get()
        .and_then(|state| {
            if let Ok(mut guard) = state.shutdown_tx.lock() {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(());
                }
            }
            state
                .completion_rx
                .lock()
                .ok()
                .and_then(|mut guard| guard.take())
                .and_then(|rx| rx.recv().ok());
            state.stats_map.read().ok().map(|stats| stats.clone())
        })
        .map(|stats| {
            let mut channels: Vec<ChannelEntry> = stats.into_values().collect();
            channels.sort_by(compare_channel_entries);
            channels
        })
        .unwrap_or_default()
}

pub(crate) fn report_channels_table(
    channels: &[ChannelEntry],
    elapsed: std::time::Duration,
    writer: &mut dyn Write,
) {
    if channels.is_empty() {
        return;
    }

    let _ = writeln!(
        writer,
        "\n=== Channel Statistics (runtime: {:.2}s) ===",
        elapsed.as_secs_f64()
    );

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        Cell::new("Channel"),
        Cell::new("Type"),
        Cell::new("State"),
        Cell::new("Sent"),
        Cell::new("Received"),
        Cell::new("Queued"),
        Cell::new("Max Q"),
        Cell::new("Mem"),
    ]));

    for channel_stats in channels {
        let label = resolve_label(
            channel_stats.source,
            channel_stats.label.as_deref(),
            Some(channel_stats.iter),
        );
        table.add_row(Row::new(vec![
            Cell::new(&label),
            Cell::new(&channel_stats.channel_type.to_string()),
            Cell::new(channel_stats.state.as_str()),
            Cell::new(&channel_stats.sent_count.to_string()),
            Cell::new(&channel_stats.received_count.to_string()),
            Cell::new(&channel_stats.queued().to_string()),
            Cell::new(&channel_stats.max_queued.to_string()),
            Cell::new(&format_bytes(channel_stats.queued_bytes())),
        ]));
    }

    let _ = writeln!(writer, "\nChannels:");
    let _ = table.print(writer);
}

pub(crate) fn collect_channels_json(
    channels: &[ChannelEntry],
    elapsed: std::time::Duration,
) -> JsonChannelsList {
    JsonChannelsList {
        current_elapsed_ns: elapsed.as_nanos() as u64,
        data: channels.iter().map(JsonChannelEntry::from).collect(),
    }
}

pub(crate) fn shutdown_streams() -> Vec<StreamStats> {
    STREAMS_STATE
        .get()
        .and_then(|state| {
            if let Ok(mut guard) = state.shutdown_tx.lock() {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(());
                }
            }
            state
                .completion_rx
                .lock()
                .ok()
                .and_then(|mut guard| guard.take())
                .and_then(|rx| rx.recv().ok());
            state.stats_map.read().ok().map(|stats| stats.clone())
        })
        .map(|stats| {
            let mut streams: Vec<StreamStats> = stats.into_values().collect();
            streams.sort_by(compare_stream_stats);
            streams
        })
        .unwrap_or_default()
}

pub(crate) fn report_streams_table(
    streams: &[StreamStats],
    elapsed: std::time::Duration,
    writer: &mut dyn Write,
) {
    if streams.is_empty() {
        return;
    }

    let _ = writeln!(
        writer,
        "\n=== Stream Statistics (runtime: {:.2}s) ===",
        elapsed.as_secs_f64()
    );

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        Cell::new("Stream"),
        Cell::new("State"),
        Cell::new("Yielded"),
    ]));

    for stream_stats in streams {
        let label = resolve_label(
            stream_stats.source,
            stream_stats.label.as_deref(),
            Some(stream_stats.iter),
        );
        table.add_row(Row::new(vec![
            Cell::new(&label),
            Cell::new(stream_stats.state.as_str()),
            Cell::new(&stream_stats.items_yielded.to_string()),
        ]));
    }

    let _ = writeln!(writer, "\nStreams:");
    let _ = table.print(writer);
}

pub(crate) fn collect_streams_json(
    streams: &[StreamStats],
    elapsed: std::time::Duration,
) -> JsonStreamsList {
    JsonStreamsList {
        current_elapsed_ns: elapsed.as_nanos() as u64,
        data: streams.iter().map(JsonStreamEntry::from).collect(),
    }
}

pub(crate) fn shutdown_futures() -> Vec<FutureEntry> {
    FUTURES_STATE
        .get()
        .and_then(|state| {
            if let Ok(mut guard) = state.shutdown_tx.lock() {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(());
                }
            }
            state
                .completion_rx
                .lock()
                .ok()
                .and_then(|mut guard| guard.take())
                .and_then(|rx| rx.recv().ok());
            state.stats_map.read().ok().map(|stats| stats.clone())
        })
        .map(|stats| {
            let mut futures: Vec<FutureEntry> = stats.into_values().collect();
            futures.sort_by(compare_future_stats);
            futures
        })
        .unwrap_or_default()
}

pub(crate) fn report_futures_table(
    futures: &[FutureEntry],
    elapsed: std::time::Duration,
    writer: &mut dyn Write,
) {
    if futures.is_empty() {
        return;
    }

    let _ = writeln!(
        writer,
        "\n=== Future Statistics (runtime: {:.2}s) ===",
        elapsed.as_secs_f64()
    );

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        Cell::new("Future"),
        Cell::new("Calls"),
        Cell::new("Polls"),
    ]));

    for future_stats in futures {
        let label = resolve_label(future_stats.source, future_stats.label.as_deref(), None);
        table.add_row(Row::new(vec![
            Cell::new(&label),
            Cell::new(&future_stats.logs_count.to_string()),
            Cell::new(&future_stats.total_polls().to_string()),
        ]));
    }

    let _ = writeln!(writer, "\nFutures:");
    let _ = table.print(writer);
}

pub(crate) fn collect_futures_json(
    futures: &[FutureEntry],
    elapsed: std::time::Duration,
) -> JsonFuturesList {
    JsonFuturesList {
        current_elapsed_ns: elapsed.as_nanos() as u64,
        data: futures.iter().map(JsonFutureEntry::from).collect(),
    }
}

#[cfg(feature = "threads")]
pub(crate) fn report_threads_table(elapsed: std::time::Duration, writer: &mut dyn Write) {
    let threads_json = crate::threads::get_threads_json();

    if threads_json.data.is_empty() {
        return;
    }

    let _ = writeln!(
        writer,
        "\n=== Thread Statistics (runtime: {:.2}s) ===",
        elapsed.as_secs_f64()
    );

    let has_alloc = threads_json.data.iter().any(|t| t.alloc_bytes.is_some());

    let mut header = vec![
        Cell::new("Thread"),
        Cell::new("Status"),
        Cell::new("CPU%"),
        Cell::new("CPU User"),
        Cell::new("CPU Sys"),
        Cell::new("CPU Total"),
    ];
    if has_alloc {
        header.push(Cell::new("Alloc"));
        header.push(Cell::new("Dealloc"));
        header.push(Cell::new("Diff"));
    }

    let mut table = Table::new();
    table.add_row(Row::new(header));

    for thread in &threads_json.data {
        let cpu_pct = thread.cpu_percent.as_deref().unwrap_or("-");
        let mut row = vec![
            Cell::new(&thread.name),
            Cell::new(&thread.status),
            Cell::new(cpu_pct),
            Cell::new(&thread.cpu_user),
            Cell::new(&thread.cpu_sys),
            Cell::new(&thread.cpu_total),
        ];
        if has_alloc {
            row.push(Cell::new(thread.alloc_bytes.as_deref().unwrap_or("-")));
            row.push(Cell::new(thread.dealloc_bytes.as_deref().unwrap_or("-")));
            row.push(Cell::new(thread.mem_diff.as_deref().unwrap_or("-")));
        }
        table.add_row(Row::new(row));
    }

    let mut summary_parts = Vec::new();
    if let Some(rss) = &threads_json.rss_bytes {
        summary_parts.push(format!("RSS: {}", rss));
    }
    if let Some(alloc) = &threads_json.total_alloc_bytes {
        summary_parts.push(format!("Alloc: {}", alloc));
    }
    if let Some(dealloc) = &threads_json.total_dealloc_bytes {
        summary_parts.push(format!("Dealloc: {}", dealloc));
    }
    if let Some(diff) = &threads_json.alloc_dealloc_diff {
        summary_parts.push(format!("Diff: {}", diff));
    }

    let summary = if summary_parts.is_empty() {
        String::new()
    } else {
        format!(", {}", summary_parts.join(", "))
    };

    let _ = writeln!(
        writer,
        "\nThreads ({}{}):",
        threads_json.thread_count, summary
    );
    let _ = table.print(writer);
}

#[cfg(feature = "threads")]
pub(crate) fn collect_threads_json() -> crate::json::JsonThreadsList {
    crate::threads::get_threads_json()
}
