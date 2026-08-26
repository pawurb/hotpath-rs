use std::collections::HashMap;
use std::io::Write;

use prettytable::{color, Attr, Cell, Row, Table};

use crate::channels::{
    channel_to_json, compare_channel_entries, resolve_label, ChannelEntry, CHANNELS_STATE,
};
use crate::debug::{
    get_sorted_debug_dbg_entries, get_sorted_debug_gauge_entries, get_sorted_debug_val_entries,
};
use crate::futures::{compare_future_stats, FutureEntry, FUTURES_STATE};
use crate::http::{compare_http_entries, HttpEntry, HTTP_STATE};
use crate::io::{compare_io_entries, IoEntry, IoOpKind, IoOpStats, IO_STATE};
use crate::json::JsonDebugEntry;
use crate::json::{
    JsonChannelsList, JsonFutureEntry, JsonFuturesList, JsonHttpEntry, JsonHttpList, JsonIoEntry,
    JsonIoList, JsonIoOpStats, JsonMutexEntry, JsonMutexesList, JsonRwLockEntry, JsonRwLocksList,
    JsonServerEntry, JsonServerList, JsonSqlEntry, JsonSqlList, JsonStreamEntry, JsonStreamsList,
};
use crate::mutexes::{compare_mutex_entries, MutexEntry, MUTEXES_STATE};
use crate::output::{
    format_bytes, format_duration, format_percentile_header, format_percentile_key, format_rate,
    format_throughput,
};
use crate::output_on::write_section_header;
use crate::rw_locks::{compare_rw_lock_entries, RwLockEntry, RwLockKind, RW_LOCKS_STATE};
use crate::server::{compare_server_entries, ServerEntry, SERVER_STATE};
use crate::sql::{compare_sql_entries, SqlEntry, SQL_STATE};
use crate::streams::{compare_stream_stats, StreamStats, STREAMS_STATE};

/// `-` for entries with events but no measured duration (count-only sampling).
fn format_sampled_duration(nanos: u64, sampled_count: u64, count: u64) -> String {
    if sampled_count == 0 && count > 0 {
        "-".to_string()
    } else {
        format_duration(nanos)
    }
}

fn styled_header(text: &str) -> Cell {
    if crate::output::use_colors() {
        Cell::new(text)
            .with_style(Attr::Bold)
            .with_style(Attr::ForegroundColor(color::CYAN))
    } else {
        Cell::new(text).with_style(Attr::Bold)
    }
}

fn print_table(table: &Table, writer: &mut dyn Write) {
    if crate::output::use_colors() {
        let _ = table.print_tty(false);
    } else {
        let _ = table.print(writer);
    }
}

pub(crate) fn shutdown_channels() -> Vec<ChannelEntry> {
    crate::channels::stop_channel_events();
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
            state
                .inner
                .read()
                .ok()
                .map(|inner| inner.stats.values().cloned().collect::<Vec<_>>())
        })
        .map(|mut channels| {
            channels.sort_by(compare_channel_entries);
            channels
        })
        .unwrap_or_default()
}

pub(crate) fn report_channels_table(
    channels: &[ChannelEntry],
    total_count: usize,
    elapsed: std::time::Duration,
    writer: &mut dyn Write,
) {
    let now_ns = elapsed.as_nanos() as u64;
    if channels.is_empty() {
        return;
    }

    write_section_header(writer, "channels", "Channel throughput statistics.");

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        styled_header("Channel"),
        styled_header("Type"),
        styled_header("Inst"),
        styled_header("Sent"),
        styled_header("Received"),
        styled_header("Sent/s"),
        styled_header("Recv/s"),
        styled_header("Max queue"),
    ]));

    for channel_stats in channels {
        let label = resolve_label(
            channel_stats.source,
            channel_stats.label.as_deref(),
            Some(channel_stats.iter),
        );
        // Queue depth is only tracked for `wrap = true` channels; proxy channels show `-`.
        let max_queue = channel_stats
            .max_queue_size
            .map_or_else(|| "-".to_string(), |q| q.to_string());
        table.add_row(Row::new(vec![
            Cell::new(&label),
            Cell::new(&channel_stats.channel_type.to_string()),
            Cell::new(&channel_stats.instances.to_string()),
            Cell::new(&channel_stats.sent_count.to_string()),
            Cell::new(&channel_stats.received_count.to_string()),
            Cell::new(&format_rate(channel_stats.sent_per_sec(now_ns))),
            Cell::new(&format_rate(channel_stats.received_per_sec(now_ns))),
            Cell::new(&max_queue),
        ]));
    }

    if channels.len() < total_count {
        let _ = write!(writer, " ({}/{})", channels.len(), total_count);
    }
    let _ = writeln!(writer);
    print_table(&table, writer);
    let _ = writeln!(writer);
}

pub(crate) fn report_channel_latency_table(
    channels: &[ChannelEntry],
    percentiles: &[f64],
    writer: &mut dyn Write,
) {
    let rows: Vec<&ChannelEntry> = channels
        .iter()
        .filter(|c| c.has_proc_hist() && c.received_count > 0)
        .collect();
    if rows.is_empty() {
        return;
    }

    write_section_header(
        writer,
        "channels latency",
        "Channel send->receive latency statistics.",
    );
    let _ = writeln!(writer);

    let mut header = vec![
        styled_header("Channel"),
        styled_header("Msgs"),
        styled_header("Avg"),
    ];
    for &p in percentiles {
        header.push(styled_header(&format_percentile_header(p)));
    }

    let mut table = Table::new();
    table.add_row(Row::new(header));

    for channel in rows {
        let label = resolve_label(channel.source, channel.label.as_deref(), Some(channel.iter));
        let count_only = channel.proc_sampled_count == 0;
        let duration_cell = |nanos: u64| {
            if count_only {
                Cell::new("-")
            } else {
                Cell::new(&format_duration(nanos))
            }
        };
        let mut row = vec![
            Cell::new(&label),
            Cell::new(&channel.received_count.to_string()),
            duration_cell(channel.proc_avg_nanos()),
        ];
        for &p in percentiles {
            row.push(duration_cell(channel.proc_percentile_nanos(p)));
        }
        table.add_row(Row::new(row));
    }

    print_table(&table, writer);
    let _ = writeln!(writer);
}

pub(crate) fn collect_channels_json(
    channels: &[ChannelEntry],
    elapsed: std::time::Duration,
    percentiles: &[f64],
    histograms: bool,
) -> JsonChannelsList {
    let current_elapsed_ns = elapsed.as_nanos() as u64;
    JsonChannelsList {
        current_elapsed_ns,
        percentiles: percentiles.to_vec(),
        data: channels
            .iter()
            .map(|entry| channel_to_json(entry, percentiles, current_elapsed_ns, histograms))
            .collect(),
    }
}

pub(crate) fn shutdown_rw_locks() -> Vec<RwLockEntry> {
    crate::lib_on::rw_locks::stop_rw_lock_events();
    RW_LOCKS_STATE
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
            state
                .inner
                .read()
                .ok()
                .map(|inner| inner.stats.values().cloned().collect::<Vec<_>>())
        })
        .map(|mut rw_locks| {
            rw_locks.sort_by(compare_rw_lock_entries);
            rw_locks
        })
        .unwrap_or_default()
}

pub(crate) fn report_rw_locks_table(
    rw_locks: &[RwLockEntry],
    total_count: usize,
    percentiles: &[f64],
    writer: &mut dyn Write,
) {
    if rw_locks.is_empty() {
        return;
    }

    write_section_header(writer, "rw_locks", "RwLock wait & acquire time statistics.");
    if rw_locks.len() < total_count {
        let _ = write!(writer, " ({}/{})", rw_locks.len(), total_count);
    }
    let _ = writeln!(writer);

    report_rw_locks_subtable(rw_locks, RwLockKind::Read, percentiles, writer);
    report_rw_locks_subtable(rw_locks, RwLockKind::Write, percentiles, writer);
}

fn report_rw_locks_subtable(
    rw_locks: &[RwLockEntry],
    kind: RwLockKind,
    percentiles: &[f64],
    writer: &mut dyn Write,
) {
    let rows: Vec<&RwLockEntry> = rw_locks.iter().filter(|l| l.count(kind) > 0).collect();
    if rows.is_empty() {
        return;
    }

    let count_label = match kind {
        RwLockKind::Read => "Reads",
        RwLockKind::Write => "Writes",
    };

    let mut header = vec![
        styled_header("RwLock"),
        styled_header(count_label),
        styled_header("Wait avg"),
    ];
    for &p in percentiles {
        header.push(styled_header(&format!(
            "Wait {}",
            format_percentile_header(p)
        )));
    }
    header.push(styled_header("Acq avg"));
    for &p in percentiles {
        header.push(styled_header(&format!(
            "Acq {}",
            format_percentile_header(p)
        )));
    }

    let mut table = Table::new();
    table.add_row(Row::new(header));

    for rw_lock in rows {
        let label = resolve_label(rw_lock.source, rw_lock.label.as_deref(), Some(rw_lock.iter));
        let fmt = |nanos: u64| {
            format_sampled_duration(nanos, rw_lock.sampled_count(kind), rw_lock.count(kind))
        };
        let mut row = vec![
            Cell::new(&label),
            Cell::new(&rw_lock.count(kind).to_string()),
            Cell::new(&fmt(rw_lock.wait_avg_nanos(kind))),
        ];
        for &p in percentiles {
            row.push(Cell::new(&fmt(rw_lock.wait_percentile_nanos(kind, p))));
        }
        row.push(Cell::new(&fmt(rw_lock.acquire_avg_nanos(kind))));
        for &p in percentiles {
            row.push(Cell::new(&fmt(rw_lock.acquire_percentile_nanos(kind, p))));
        }
        table.add_row(Row::new(row));
    }

    print_table(&table, writer);
    let _ = writeln!(writer);
}

fn rw_lock_to_json(
    rw_lock: &RwLockEntry,
    percentiles: &[f64],
    histograms: bool,
) -> JsonRwLockEntry {
    let label = resolve_label(rw_lock.source, rw_lock.label.as_deref(), Some(rw_lock.iter));

    let fmt = |kind: RwLockKind, nanos: u64| {
        format_sampled_duration(nanos, rw_lock.sampled_count(kind), rw_lock.count(kind))
    };
    let mut read_wait_percentiles = HashMap::new();
    let mut write_wait_percentiles = HashMap::new();
    let mut read_acquire_percentiles = HashMap::new();
    let mut write_acquire_percentiles = HashMap::new();
    for &p in percentiles {
        let key = format_percentile_key(p);
        read_wait_percentiles.insert(
            key.clone(),
            fmt(
                RwLockKind::Read,
                rw_lock.wait_percentile_nanos(RwLockKind::Read, p),
            ),
        );
        write_wait_percentiles.insert(
            key.clone(),
            fmt(
                RwLockKind::Write,
                rw_lock.wait_percentile_nanos(RwLockKind::Write, p),
            ),
        );
        read_acquire_percentiles.insert(
            key.clone(),
            fmt(
                RwLockKind::Read,
                rw_lock.acquire_percentile_nanos(RwLockKind::Read, p),
            ),
        );
        write_acquire_percentiles.insert(
            key,
            fmt(
                RwLockKind::Write,
                rw_lock.acquire_percentile_nanos(RwLockKind::Write, p),
            ),
        );
    }

    JsonRwLockEntry {
        id: rw_lock.id,
        source: rw_lock.source.to_string(),
        label,
        has_custom_label: rw_lock.label.is_some(),
        type_name: rw_lock.type_name.to_string(),
        read_count: rw_lock.read_count,
        write_count: rw_lock.write_count,
        read_sampled_count: rw_lock.read_sampled_count,
        write_sampled_count: rw_lock.write_sampled_count,
        read_wait_avg: fmt(RwLockKind::Read, rw_lock.wait_avg_nanos(RwLockKind::Read)),
        write_wait_avg: fmt(RwLockKind::Write, rw_lock.wait_avg_nanos(RwLockKind::Write)),
        read_acquire_avg: fmt(
            RwLockKind::Read,
            rw_lock.acquire_avg_nanos(RwLockKind::Read),
        ),
        write_acquire_avg: fmt(
            RwLockKind::Write,
            rw_lock.acquire_avg_nanos(RwLockKind::Write),
        ),
        read_wait_percentiles,
        write_wait_percentiles,
        read_acquire_percentiles,
        write_acquire_percentiles,
        read_wait_histogram: histograms
            .then(|| rw_lock.wait_histogram_base64(RwLockKind::Read))
            .flatten(),
        write_wait_histogram: histograms
            .then(|| rw_lock.wait_histogram_base64(RwLockKind::Write))
            .flatten(),
        read_acquire_histogram: histograms
            .then(|| rw_lock.acquire_histogram_base64(RwLockKind::Read))
            .flatten(),
        write_acquire_histogram: histograms
            .then(|| rw_lock.acquire_histogram_base64(RwLockKind::Write))
            .flatten(),
        iter: rw_lock.iter,
    }
}

pub(crate) fn collect_rw_locks_json(
    rw_locks: &[RwLockEntry],
    elapsed: std::time::Duration,
    percentiles: &[f64],
    histograms: bool,
) -> JsonRwLocksList {
    JsonRwLocksList {
        current_elapsed_ns: elapsed.as_nanos() as u64,
        percentiles: percentiles.to_vec(),
        data: rw_locks
            .iter()
            .map(|rw_lock| rw_lock_to_json(rw_lock, percentiles, histograms))
            .collect(),
    }
}

pub(crate) fn shutdown_mutexes() -> Vec<MutexEntry> {
    crate::lib_on::mutexes::stop_mutex_events();
    MUTEXES_STATE
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
            state
                .inner
                .read()
                .ok()
                .map(|inner| inner.stats.values().cloned().collect::<Vec<_>>())
        })
        .map(|mut mutexes| {
            mutexes.sort_by(compare_mutex_entries);
            mutexes
        })
        .unwrap_or_default()
}

pub(crate) fn report_mutexes_table(
    mutexes: &[MutexEntry],
    total_count: usize,
    percentiles: &[f64],
    writer: &mut dyn Write,
) {
    let rows: Vec<&MutexEntry> = mutexes.iter().filter(|l| l.count > 0).collect();
    if rows.is_empty() {
        return;
    }

    write_section_header(writer, "mutexes", "Mutex wait & acquire time statistics.");
    if mutexes.len() < total_count {
        let _ = write!(writer, " ({}/{})", mutexes.len(), total_count);
    }
    let _ = writeln!(writer);

    let mut header = vec![
        styled_header("Mutex"),
        styled_header("Locks"),
        styled_header("Wait avg"),
    ];
    for &p in percentiles {
        header.push(styled_header(&format!(
            "Wait {}",
            format_percentile_header(p)
        )));
    }
    header.push(styled_header("Acq avg"));
    for &p in percentiles {
        header.push(styled_header(&format!(
            "Acq {}",
            format_percentile_header(p)
        )));
    }

    let mut table = Table::new();
    table.add_row(Row::new(header));

    for mutex in rows {
        let label = resolve_label(mutex.source, mutex.label.as_deref(), Some(mutex.iter));
        let fmt = |nanos: u64| format_sampled_duration(nanos, mutex.sampled_count, mutex.count);
        let mut row = vec![
            Cell::new(&label),
            Cell::new(&mutex.count.to_string()),
            Cell::new(&fmt(mutex.wait_avg_nanos())),
        ];
        for &p in percentiles {
            row.push(Cell::new(&fmt(mutex.wait_percentile_nanos(p))));
        }
        row.push(Cell::new(&fmt(mutex.acquire_avg_nanos())));
        for &p in percentiles {
            row.push(Cell::new(&fmt(mutex.acquire_percentile_nanos(p))));
        }
        table.add_row(Row::new(row));
    }

    print_table(&table, writer);
    let _ = writeln!(writer);
}

fn mutex_to_json(mutex: &MutexEntry, percentiles: &[f64], histograms: bool) -> JsonMutexEntry {
    let label = resolve_label(mutex.source, mutex.label.as_deref(), Some(mutex.iter));

    let fmt = |nanos: u64| format_sampled_duration(nanos, mutex.sampled_count, mutex.count);
    let mut wait_percentiles = HashMap::new();
    let mut acquire_percentiles = HashMap::new();
    for &p in percentiles {
        let key = format_percentile_key(p);
        wait_percentiles.insert(key.clone(), fmt(mutex.wait_percentile_nanos(p)));
        acquire_percentiles.insert(key, fmt(mutex.acquire_percentile_nanos(p)));
    }

    JsonMutexEntry {
        id: mutex.id,
        source: mutex.source.to_string(),
        label,
        has_custom_label: mutex.label.is_some(),
        type_name: mutex.type_name.to_string(),
        count: mutex.count,
        sampled_count: mutex.sampled_count,
        wait_avg: fmt(mutex.wait_avg_nanos()),
        acquire_avg: fmt(mutex.acquire_avg_nanos()),
        wait_percentiles,
        acquire_percentiles,
        wait_histogram: histograms.then(|| mutex.wait_histogram_base64()).flatten(),
        acquire_histogram: histograms
            .then(|| mutex.acquire_histogram_base64())
            .flatten(),
        iter: mutex.iter,
    }
}

pub(crate) fn collect_mutexes_json(
    mutexes: &[MutexEntry],
    elapsed: std::time::Duration,
    percentiles: &[f64],
    histograms: bool,
) -> JsonMutexesList {
    JsonMutexesList {
        current_elapsed_ns: elapsed.as_nanos() as u64,
        percentiles: percentiles.to_vec(),
        data: mutexes
            .iter()
            .map(|mutex| mutex_to_json(mutex, percentiles, histograms))
            .collect(),
    }
}

pub(crate) fn shutdown_sql() -> Vec<SqlEntry> {
    crate::lib_on::sql::stop_sql_events();
    SQL_STATE
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
            state
                .inner
                .read()
                .ok()
                .map(|inner| inner.stats.values().cloned().collect::<Vec<_>>())
        })
        .map(|mut entries| {
            entries.sort_by(compare_sql_entries);
            entries
        })
        .unwrap_or_default()
}

const SQL_QUERY_DISPLAY_LEN: usize = 60;

fn truncate_query(query: &str) -> String {
    if query.chars().count() <= SQL_QUERY_DISPLAY_LEN {
        return query.to_string();
    }
    let truncated: String = query.chars().take(SQL_QUERY_DISPLAY_LEN - 3).collect();
    format!("{}...", truncated)
}

pub(crate) fn report_sql_table(
    entries: &[SqlEntry],
    total_count: usize,
    total_calls: u64,
    reference_total: u64,
    percentiles: &[f64],
    writer: &mut dyn Write,
) {
    if entries.is_empty() {
        return;
    }

    write_section_header(writer, "sql", "SQL query execution time statistics.");
    if entries.len() < total_count {
        let _ = write!(writer, " ({}/{})", entries.len(), total_count);
    }
    let _ = writeln!(writer);
    let _ = writeln!(writer, "Total calls: {}", total_calls);

    let show_route = entries.iter().any(|e| e.route.is_some());
    let mut header = vec![styled_header("Query"), styled_header("Source")];
    if show_route {
        header.push(styled_header("Route"));
    }
    header.extend([styled_header("Calls"), styled_header("Avg")]);
    for &p in percentiles {
        header.push(styled_header(&format_percentile_header(p)));
    }
    header.push(styled_header("Total"));
    header.push(styled_header("% Total"));

    let mut table = Table::new();
    table.add_row(Row::new(header));

    for entry in entries {
        let mut row = vec![
            Cell::new(&truncate_query(&entry.query)),
            Cell::new(&format_source(entry.source)),
        ];
        if show_route {
            row.push(Cell::new(&format_route(entry.route)));
        }
        row.extend([
            Cell::new(&entry.count.to_string()),
            Cell::new(&format_duration(entry.avg_nanos())),
        ]);
        for &p in percentiles {
            row.push(Cell::new(&format_duration(entry.percentile_nanos(p))));
        }
        row.push(Cell::new(&format_duration(entry.total_nanos)));
        row.push(Cell::new(&format_sql_percent(
            entry.total_nanos,
            reference_total,
        )));
        table.add_row(Row::new(row));
    }

    print_table(&table, writer);
    let _ = writeln!(writer);
}

fn format_source(source: Option<&'static str>) -> String {
    source.map_or_else(|| "-".to_string(), crate::output::shorten_function_name)
}

fn format_route(route: Option<&'static str>) -> String {
    route.unwrap_or("-").to_string()
}

fn format_sql_percent(total_nanos: u64, reference_total: u64) -> String {
    let percentage = if reference_total > 0 {
        (total_nanos as f64 / reference_total as f64) * 100.0
    } else {
        0.0
    };
    format!("{:.2}%", percentage)
}

fn sql_to_json(
    entry: &SqlEntry,
    reference_total: u64,
    percentiles: &[f64],
    histograms: bool,
) -> JsonSqlEntry {
    let mut percentile_map = HashMap::new();
    for &p in percentiles {
        percentile_map.insert(
            format_percentile_key(p),
            format_duration(entry.percentile_nanos(p)),
        );
    }

    JsonSqlEntry {
        id: entry.id,
        query: entry.query.clone(),
        source: entry.source.map(String::from),
        route: entry.route.map(String::from),
        count: entry.count,
        avg: format_duration(entry.avg_nanos()),
        total: format_duration(entry.total_nanos),
        percent_total: format_sql_percent(entry.total_nanos, reference_total),
        percentiles: percentile_map,
        histogram: histograms.then(|| entry.histogram_base64()).flatten(),
    }
}

pub(crate) fn collect_sql_json(
    entries: &[SqlEntry],
    elapsed: std::time::Duration,
    total_calls: u64,
    reference_total: u64,
    percentiles: &[f64],
    histograms: bool,
) -> JsonSqlList {
    JsonSqlList {
        current_elapsed_ns: elapsed.as_nanos() as u64,
        total_ns: reference_total,
        total_calls,
        percentiles: percentiles.to_vec(),
        data: entries
            .iter()
            .map(|entry| sql_to_json(entry, reference_total, percentiles, histograms))
            .collect(),
    }
}

pub(crate) fn shutdown_http() -> Vec<HttpEntry> {
    crate::lib_on::http::stop_http_events();
    HTTP_STATE
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
            state
                .inner
                .read()
                .ok()
                .map(|inner| inner.stats.values().cloned().collect::<Vec<_>>())
        })
        .map(|mut entries| {
            entries.sort_by(compare_http_entries);
            entries
        })
        .unwrap_or_default()
}

pub(crate) fn report_http_table(
    entries: &[HttpEntry],
    total_count: usize,
    total_calls: u64,
    reference_total: u64,
    percentiles: &[f64],
    writer: &mut dyn Write,
) {
    if entries.is_empty() {
        return;
    }

    write_section_header(writer, "http", "HTTP request execution time statistics.");
    if entries.len() < total_count {
        let _ = write!(writer, " ({}/{})", entries.len(), total_count);
    }
    let _ = writeln!(writer);
    let _ = writeln!(writer, "Total calls: {}", total_calls);

    let show_route = entries.iter().any(|e| e.route.is_some());
    let mut header = vec![styled_header("Endpoint"), styled_header("Source")];
    if show_route {
        header.push(styled_header("Route"));
    }
    header.extend([
        styled_header("Calls"),
        styled_header("Errors"),
        styled_header("Avg"),
    ]);
    for &p in percentiles {
        header.push(styled_header(&format_percentile_header(p)));
    }
    header.push(styled_header("Total"));
    header.push(styled_header("% Total"));

    let mut table = Table::new();
    table.add_row(Row::new(header));

    for entry in entries {
        let mut row = vec![
            Cell::new(&truncate_query(&entry.endpoint)),
            Cell::new(&format_source(entry.source)),
        ];
        if show_route {
            row.push(Cell::new(&format_route(entry.route)));
        }
        row.extend([
            Cell::new(&entry.count.to_string()),
            Cell::new(&entry.error_count.to_string()),
            Cell::new(&format_duration(entry.avg_nanos())),
        ]);
        for &p in percentiles {
            row.push(Cell::new(&format_duration(entry.percentile_nanos(p))));
        }
        row.push(Cell::new(&format_duration(entry.total_nanos)));
        row.push(Cell::new(&format_sql_percent(
            entry.total_nanos,
            reference_total,
        )));
        table.add_row(Row::new(row));
    }

    print_table(&table, writer);
    let _ = writeln!(writer);
}

fn http_to_json(
    entry: &HttpEntry,
    reference_total: u64,
    percentiles: &[f64],
    histograms: bool,
) -> JsonHttpEntry {
    let mut percentile_map = HashMap::new();
    for &p in percentiles {
        percentile_map.insert(
            format_percentile_key(p),
            format_duration(entry.percentile_nanos(p)),
        );
    }

    JsonHttpEntry {
        id: entry.id,
        endpoint: entry.endpoint.clone(),
        source: entry.source.map(String::from),
        route: entry.route.map(String::from),
        count: entry.count,
        errors: entry.error_count,
        avg: format_duration(entry.avg_nanos()),
        total: format_duration(entry.total_nanos),
        percent_total: format_sql_percent(entry.total_nanos, reference_total),
        percentiles: percentile_map,
        histogram: histograms.then(|| entry.histogram_base64()).flatten(),
    }
}

pub(crate) fn collect_http_json(
    entries: &[HttpEntry],
    elapsed: std::time::Duration,
    total_calls: u64,
    reference_total: u64,
    percentiles: &[f64],
    histograms: bool,
) -> JsonHttpList {
    JsonHttpList {
        current_elapsed_ns: elapsed.as_nanos() as u64,
        total_ns: reference_total,
        total_calls,
        percentiles: percentiles.to_vec(),
        data: entries
            .iter()
            .map(|entry| http_to_json(entry, reference_total, percentiles, histograms))
            .collect(),
    }
}

pub(crate) fn shutdown_server() -> Vec<ServerEntry> {
    crate::lib_on::server::stop_server_events();
    SERVER_STATE
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
            state
                .inner
                .read()
                .ok()
                .map(|inner| inner.stats.values().cloned().collect::<Vec<_>>())
        })
        .map(|mut entries| {
            entries.sort_by(compare_server_entries);
            entries
        })
        .unwrap_or_default()
}

/// Which per-request columns the server table shows: each only when the
/// corresponding subsystem initialized, so an app without SQL profiling does
/// not get an all-`-` column.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ServerColumns {
    pub(crate) sql: bool,
    pub(crate) http: bool,
}

impl ServerColumns {
    pub(crate) fn from_state() -> Self {
        Self {
            sql: SQL_STATE.get().is_some(),
            http: HTTP_STATE.get().is_some(),
        }
    }
}

fn format_per_request(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_string(), |v| format!("{v:.1}"))
}

pub(crate) fn report_server_table(
    entries: &[ServerEntry],
    total_count: usize,
    total_calls: u64,
    reference_total: u64,
    percentiles: &[f64],
    columns: ServerColumns,
    writer: &mut dyn Write,
) {
    if entries.is_empty() {
        return;
    }

    write_section_header(
        writer,
        "server",
        "HTTP server response time statistics per route.",
    );
    if entries.len() < total_count {
        let _ = write!(writer, " ({}/{})", entries.len(), total_count);
    }
    let _ = writeln!(writer);
    let _ = writeln!(writer, "Total requests: {}", total_calls);

    let mut header = vec![
        styled_header("Route"),
        styled_header("Calls"),
        styled_header("4xx"),
        styled_header("5xx"),
    ];
    if columns.sql {
        header.push(styled_header("SQL/req"));
    }
    if columns.http {
        header.push(styled_header("HTTP/req"));
    }
    header.push(styled_header("Avg"));
    for &p in percentiles {
        header.push(styled_header(&format_percentile_header(p)));
    }
    header.push(styled_header("Total"));
    header.push(styled_header("% Total"));

    let mut table = Table::new();
    table.add_row(Row::new(header));

    for entry in entries {
        let mut row = vec![
            Cell::new(&truncate_query(&entry.route)),
            Cell::new(&entry.count.to_string()),
            Cell::new(&entry.status_4xx.to_string()),
            Cell::new(&entry.status_5xx.to_string()),
        ];
        if columns.sql {
            row.push(Cell::new(&format_per_request(entry.sql_per_request())));
        }
        if columns.http {
            row.push(Cell::new(&format_per_request(entry.http_per_request())));
        }
        row.push(Cell::new(&format_duration(entry.avg_nanos())));
        for &p in percentiles {
            row.push(Cell::new(&format_duration(entry.percentile_nanos(p))));
        }
        row.push(Cell::new(&format_duration(entry.total_nanos)));
        row.push(Cell::new(&format_sql_percent(
            entry.total_nanos,
            reference_total,
        )));
        table.add_row(Row::new(row));
    }

    print_table(&table, writer);
    let _ = writeln!(writer);
}

fn server_to_json(
    entry: &ServerEntry,
    reference_total: u64,
    percentiles: &[f64],
    columns: ServerColumns,
    histograms: bool,
) -> JsonServerEntry {
    let mut percentile_map = HashMap::new();
    for &p in percentiles {
        percentile_map.insert(
            format_percentile_key(p),
            format_duration(entry.percentile_nanos(p)),
        );
    }

    JsonServerEntry {
        id: entry.id,
        route: entry.route.clone(),
        count: entry.count,
        status_4xx: entry.status_4xx,
        status_5xx: entry.status_5xx,
        sql_per_request: columns.sql.then(|| entry.sql_per_request()).flatten(),
        http_per_request: columns.http.then(|| entry.http_per_request()).flatten(),
        avg: format_duration(entry.avg_nanos()),
        total: format_duration(entry.total_nanos),
        percent_total: format_sql_percent(entry.total_nanos, reference_total),
        percentiles: percentile_map,
        histogram: histograms.then(|| entry.histogram_base64()).flatten(),
    }
}

pub(crate) fn collect_server_json(
    entries: &[ServerEntry],
    elapsed: std::time::Duration,
    total_calls: u64,
    reference_total: u64,
    percentiles: &[f64],
    columns: ServerColumns,
    histograms: bool,
) -> JsonServerList {
    JsonServerList {
        current_elapsed_ns: elapsed.as_nanos() as u64,
        total_ns: reference_total,
        total_calls,
        percentiles: percentiles.to_vec(),
        data: entries
            .iter()
            .map(|entry| server_to_json(entry, reference_total, percentiles, columns, histograms))
            .collect(),
    }
}

pub(crate) fn shutdown_io() -> Vec<IoEntry> {
    crate::lib_on::io::stop_io_events();
    IO_STATE
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
            state
                .inner
                .read()
                .ok()
                .map(|inner| inner.stats.values().cloned().collect::<Vec<_>>())
        })
        .map(|mut entries| {
            entries.sort_by(compare_io_entries);
            entries
        })
        .unwrap_or_default()
}

pub(crate) fn report_io_table(
    entries: &[IoEntry],
    total_count: usize,
    percentiles: &[f64],
    writer: &mut dyn Write,
) {
    if entries.is_empty() {
        return;
    }

    write_section_header(writer, "io", "Byte-level I/O statistics.");
    if entries.len() < total_count {
        let _ = write!(writer, " ({}/{})", entries.len(), total_count);
    }
    let _ = writeln!(writer);

    report_io_subtable(entries, IoOpKind::Read, percentiles, writer);
    report_io_subtable(entries, IoOpKind::Write, percentiles, writer);
}

/// Reads and writes render as stacked sub-tables. The write sub-table carries
/// the flush count; shutdown operations appear only in JSON.
fn report_io_subtable(
    entries: &[IoEntry],
    kind: IoOpKind,
    percentiles: &[f64],
    writer: &mut dyn Write,
) {
    let rows: Vec<&IoEntry> = entries
        .iter()
        .filter(|e| match kind {
            IoOpKind::Read => e.read.count > 0 || e.read.errors > 0,
            _ => {
                e.write.count > 0
                    || e.flush.count > 0
                    || e.shutdown.count > 0
                    || e.write_side_errors() > 0
            }
        })
        .collect();
    if rows.is_empty() {
        return;
    }

    let count_label = match kind {
        IoOpKind::Read => "Reads",
        _ => "Writes",
    };

    let mut header = vec![
        styled_header("Io"),
        styled_header("Inst"),
        styled_header(count_label),
        styled_header("Bytes"),
        styled_header("Rate"),
        styled_header("Avg"),
    ];
    for &p in percentiles {
        header.push(styled_header(&format_percentile_header(p)));
    }
    header.push(styled_header("Total"));
    if kind == IoOpKind::Write {
        header.push(styled_header("Flushes"));
    }
    header.push(styled_header("Errors"));

    let mut table = Table::new();
    table.add_row(Row::new(header));

    for entry in rows {
        let label = resolve_label(entry.source, entry.label.as_deref(), Some(entry.iter));
        let stats = entry.op(kind);
        let fmt = |nanos: u64| format_sampled_duration(nanos, stats.sampled_count, stats.count);
        let mut row = vec![
            Cell::new(&label),
            Cell::new(&entry.instances.to_string()),
            Cell::new(&stats.count.to_string()),
            Cell::new(&format_bytes(stats.bytes)),
            Cell::new(&format_throughput(stats.throughput_bytes_per_sec())),
            Cell::new(&fmt(stats.avg_nanos())),
        ];
        for &p in percentiles {
            row.push(Cell::new(&fmt(stats.percentile_nanos(p))));
        }
        row.push(Cell::new(&fmt(stats.total_nanos)));
        let errors = if kind == IoOpKind::Write {
            row.push(Cell::new(&entry.flush.count.to_string()));
            entry.write_side_errors()
        } else {
            stats.errors
        };
        row.push(Cell::new(&errors.to_string()));
        table.add_row(Row::new(row));
    }

    print_table(&table, writer);
    let _ = writeln!(writer);
}

fn io_op_stats_to_json(stats: &IoOpStats, percentiles: &[f64], histograms: bool) -> JsonIoOpStats {
    let fmt = |nanos: u64| format_sampled_duration(nanos, stats.sampled_count, stats.count);
    let mut percentile_map = HashMap::new();
    for &p in percentiles {
        percentile_map.insert(format_percentile_key(p), fmt(stats.percentile_nanos(p)));
    }

    JsonIoOpStats {
        count: stats.count,
        sampled_count: stats.sampled_count,
        bytes: stats.bytes,
        sampled_bytes: stats.sampled_bytes,
        errors: stats.errors,
        avg: fmt(stats.avg_nanos()),
        throughput: stats
            .throughput_bytes_per_sec()
            .map(|rate| format_throughput(Some(rate))),
        total_ns: stats.total_nanos,
        percentiles: percentile_map,
        histogram: histograms.then(|| stats.histogram_base64()).flatten(),
    }
}

fn io_to_json(entry: &IoEntry, percentiles: &[f64], histograms: bool) -> JsonIoEntry {
    let label = resolve_label(entry.source, entry.label.as_deref(), Some(entry.iter));

    JsonIoEntry {
        id: entry.id,
        source: entry.source.to_string(),
        label,
        has_custom_label: entry.label.is_some(),
        type_name: entry.type_name.to_string(),
        read: io_op_stats_to_json(&entry.read, percentiles, histograms),
        write: io_op_stats_to_json(&entry.write, percentiles, histograms),
        flush: io_op_stats_to_json(&entry.flush, percentiles, histograms),
        shutdown: io_op_stats_to_json(&entry.shutdown, percentiles, histograms),
        instances: entry.instances,
        iter: entry.iter,
    }
}

pub(crate) fn collect_io_json(
    entries: &[IoEntry],
    elapsed: std::time::Duration,
    percentiles: &[f64],
    histograms: bool,
) -> JsonIoList {
    JsonIoList {
        current_elapsed_ns: elapsed.as_nanos() as u64,
        percentiles: percentiles.to_vec(),
        data: entries
            .iter()
            .map(|entry| io_to_json(entry, percentiles, histograms))
            .collect(),
    }
}

pub(crate) fn shutdown_streams() -> Vec<StreamStats> {
    crate::streams::stop_stream_events();
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
            state
                .inner
                .read()
                .ok()
                .map(|inner| inner.stats.values().cloned().collect::<Vec<_>>())
        })
        .map(|mut streams| {
            streams.sort_by(compare_stream_stats);
            streams
        })
        .unwrap_or_default()
}

pub(crate) fn report_streams_table(
    streams: &[StreamStats],
    total_count: usize,
    writer: &mut dyn Write,
) {
    if streams.is_empty() {
        return;
    }

    write_section_header(writer, "streams", "Stream yield statistics.");

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        styled_header("Stream"),
        styled_header("Inst"),
        styled_header("State"),
        styled_header("Yielded"),
    ]));

    for stream_stats in streams {
        let label = resolve_label(
            stream_stats.source,
            stream_stats.label.as_deref(),
            Some(stream_stats.iter),
        );
        table.add_row(Row::new(vec![
            Cell::new(&label),
            Cell::new(&stream_stats.instances.to_string()),
            Cell::new(
                stream_stats
                    .display_state()
                    .map_or("-", |state| state.as_str()),
            ),
            Cell::new(&stream_stats.items_yielded.to_string()),
        ]));
    }

    if streams.len() < total_count {
        let _ = write!(writer, " ({}/{})", streams.len(), total_count);
    }
    let _ = writeln!(writer);
    print_table(&table, writer);
    let _ = writeln!(writer);
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
    crate::lib_on::futures::stop_future_events();
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
            state
                .inner
                .read()
                .ok()
                .map(|inner| inner.stats.values().cloned().collect::<Vec<_>>())
        })
        .map(|mut futures| {
            futures.sort_by(compare_future_stats);
            futures
        })
        .unwrap_or_default()
}

pub(crate) fn report_futures_table(
    futures: &[FutureEntry],
    total_count: usize,
    writer: &mut dyn Write,
) {
    if futures.is_empty() {
        return;
    }

    write_section_header(writer, "futures", "Future poll and lifecycle statistics.");

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        styled_header("Future"),
        styled_header("Calls"),
        styled_header("Polls"),
        styled_header("Avg Poll"),
        styled_header("Total Poll"),
        styled_header("Avg Alloc"),
        styled_header("Total Alloc"),
    ]));

    for future_stats in futures {
        let label = resolve_label(future_stats.source, future_stats.label.as_deref(), None);
        let total_calls = future_stats.logs_count;
        let total_polls = future_stats.total_polls();
        let total_poll_dur = future_stats.display_total_poll_duration_ns();
        let total_alloc_bytes_across_polls = future_stats.total_poll_alloc_bytes();
        let avg_poll = match future_stats.avg_poll_duration_ns() {
            Some(avg) => format_duration(avg),
            None => "-".to_string(),
        };
        let avg_alloc_per_call = match total_alloc_bytes_across_polls {
            Some(total_alloc_bytes) if total_calls > 0 => {
                format_bytes(total_alloc_bytes / total_calls)
            }
            _ => "-".to_string(),
        };
        let total_alloc = total_alloc_bytes_across_polls
            .map(format_bytes)
            .unwrap_or_else(|| "-".to_string());
        let total_poll_dur = if future_stats.sampled_polls == 0 && total_polls > 0 {
            "-".to_string()
        } else {
            format_duration(total_poll_dur)
        };
        table.add_row(Row::new(vec![
            Cell::new(&label),
            Cell::new(&total_calls.to_string()),
            Cell::new(&total_polls.to_string()),
            Cell::new(&avg_poll),
            Cell::new(&total_poll_dur),
            Cell::new(&avg_alloc_per_call),
            Cell::new(&total_alloc),
        ]));
    }

    if futures.len() < total_count {
        let _ = write!(writer, " ({}/{})", futures.len(), total_count);
    }
    let _ = writeln!(writer);
    print_table(&table, writer);
    let _ = writeln!(writer);
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
pub(crate) fn report_threads_table(writer: &mut dyn Write, limit: usize) {
    let mut threads_json = crate::threads::get_threads_json();

    if threads_json.data.is_empty() {
        return;
    }

    let total_count = threads_json.data.len();
    if limit > 0 && limit < total_count {
        threads_json.data.truncate(limit);
    }

    write_section_header(writer, "threads", "Thread CPU and memory statistics.");

    let has_alloc = threads_json.data.iter().any(|t| t.alloc_bytes.is_some());

    let mut header = vec![
        styled_header("Thread"),
        styled_header("Status"),
        styled_header("CPU%"),
        styled_header("Max%"),
        styled_header("Avg%"),
    ];
    if has_alloc {
        header.push(styled_header("Alloc"));
        header.push(styled_header("Dealloc"));
        header.push(styled_header("Diff"));
    }

    let mut table = Table::new();
    table.add_row(Row::new(header));

    for thread in &threads_json.data {
        let cpu_pct = thread.cpu_percent.as_deref().unwrap_or("-");
        let cpu_pct_max = thread.cpu_percent_max.as_deref().unwrap_or("-");
        let cpu_pct_avg = thread.cpu_percent_avg.as_deref().unwrap_or("-");
        let mut row = vec![
            Cell::new(&thread.name),
            Cell::new(&thread.status),
            Cell::new(cpu_pct),
            Cell::new(cpu_pct_max),
            Cell::new(cpu_pct_avg),
        ];
        if has_alloc {
            row.push(Cell::new(thread.alloc_bytes.as_deref().unwrap_or("-")));
            row.push(Cell::new(thread.dealloc_bytes.as_deref().unwrap_or("-")));
            row.push(Cell::new(thread.mem_diff.as_deref().unwrap_or("-")));
        }
        table.add_row(Row::new(row));
    }

    let mut info_parts = Vec::new();
    if let Some(rss) = &threads_json.rss_bytes {
        info_parts.push(format!("RSS: {}", rss));
    }
    if let Some(alloc) = &threads_json.total_alloc_bytes {
        info_parts.push(format!("Alloc: {}", alloc));
    }
    if let Some(dealloc) = &threads_json.total_dealloc_bytes {
        info_parts.push(format!("Dealloc: {}", dealloc));
    }
    if let Some(diff) = &threads_json.alloc_dealloc_diff {
        info_parts.push(format!("Diff: {}", diff));
    }

    let displayed = threads_json.data.len();
    if displayed < total_count {
        info_parts.push(format!("{}/{}", displayed, total_count));
    }

    if !info_parts.is_empty() {
        let _ = write!(writer, " ({})", info_parts.join(", "));
    }
    let _ = writeln!(writer);
    print_table(&table, writer);
    let _ = writeln!(writer);
}

#[cfg(feature = "threads")]
pub(crate) fn collect_threads_json(limit: usize) -> crate::json::JsonThreadsList {
    let mut json = crate::threads::get_threads_json();
    if limit > 0 && limit < json.data.len() {
        json.data.truncate(limit);
    }
    json
}

pub(crate) fn has_debug_entries() -> bool {
    !get_sorted_debug_dbg_entries().is_empty()
        || !get_sorted_debug_val_entries().is_empty()
        || !get_sorted_debug_gauge_entries().is_empty()
}

pub(crate) fn report_debug_table(writer: &mut dyn Write) {
    let dbg_entries = get_sorted_debug_dbg_entries();
    let val_entries = get_sorted_debug_val_entries();
    let gauge_entries = get_sorted_debug_gauge_entries();

    if dbg_entries.is_empty() && val_entries.is_empty() && gauge_entries.is_empty() {
        return;
    }

    write_section_header(writer, "debug", "Debug last values (dbg!, val!, gauge!).");

    let header = vec![
        styled_header("Type"),
        styled_header("Key/Expr"),
        styled_header("Value"),
        styled_header("Updates"),
        styled_header("Source"),
    ];

    let mut table = Table::new();
    table.add_row(Row::new(header));

    let mut entries: Vec<JsonDebugEntry> = Vec::new();
    entries.extend(dbg_entries.iter().map(JsonDebugEntry::from));
    entries.extend(val_entries.iter().map(JsonDebugEntry::from));
    entries.extend(gauge_entries.iter().map(JsonDebugEntry::from));

    for entry in &entries {
        let value = entry.last_value.as_deref().unwrap_or("-");
        table.add_row(Row::new(vec![
            Cell::new(entry.entry_type.as_str()),
            Cell::new(&entry.expression),
            Cell::new(value),
            Cell::new(&entry.log_count.to_string()),
            Cell::new(&entry.source_display),
        ]));
    }

    let _ = writeln!(writer);
    print_table(&table, writer);
    let _ = writeln!(writer);
}

pub(crate) fn collect_debug_json(elapsed: std::time::Duration) -> crate::json::JsonDebugList {
    let mut entries: Vec<JsonDebugEntry> = Vec::new();
    entries.extend(
        get_sorted_debug_dbg_entries()
            .iter()
            .map(JsonDebugEntry::from),
    );
    entries.extend(
        get_sorted_debug_val_entries()
            .iter()
            .map(JsonDebugEntry::from),
    );
    entries.extend(
        get_sorted_debug_gauge_entries()
            .iter()
            .map(JsonDebugEntry::from),
    );

    crate::json::JsonDebugList {
        current_elapsed_ns: elapsed.as_nanos() as u64,
        entries,
    }
}

#[cfg(test)]
mod tests {
    use crate::lib_on::report::format_per_request;

    #[test]
    fn per_request_formatting() {
        assert_eq!(format_per_request(None), "-");
        assert_eq!(format_per_request(Some(2.0)), "2.0");
        assert_eq!(format_per_request(Some(1.25)), "1.2");
    }
}
