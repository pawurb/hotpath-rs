use hotpath::channels::ChannelType;
use ratatui::{
    style::{Color, Style},
    widgets::Cell,
};

pub(crate) fn truncate_left(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let truncated_len = max_len.saturating_sub(3);
        let start_idx = s.len().saturating_sub(truncated_len);
        format!("...{}", &s[start_idx..])
    }
}

pub(crate) fn queue_status(
    queued: u64,
    channel_type: &ChannelType,
    _width: usize,
) -> Cell<'static> {
    let capacity = match channel_type {
        ChannelType::Bounded(cap) => Some(*cap),
        ChannelType::Oneshot => Some(1),
        ChannelType::Unbounded => None,
    };

    match capacity {
        Some(cap) if cap > 0 => {
            let percentage = (queued as f64 / cap as f64 * 100.0).min(100.0);

            let text = format!("[{}/{}]", queued, cap);

            let color = if percentage >= 100.0 {
                Color::Red
            } else if percentage >= 50.0 {
                Color::Yellow
            } else {
                Color::Green
            };

            Cell::from(text).style(Style::default().fg(color))
        }
        _ => Cell::from("N/A"),
    }
}

pub(crate) fn format_delay(delay_ns: u64) -> String {
    if delay_ns < 1_000 {
        format!("{}ns", delay_ns)
    } else if delay_ns < 1_000_000 {
        format!("{:.1}μs", delay_ns as f64 / 1_000.0)
    } else if delay_ns < 1_000_000_000 {
        format!("{:.2}ms", delay_ns as f64 / 1_000_000.0)
    } else {
        format!("{:.3}s", delay_ns as f64 / 1_000_000_000.0)
    }
}

pub(crate) fn truncate_message(msg: &str, max_len: usize) -> String {
    if msg.len() <= max_len {
        format!("{:<width$}", msg, width = max_len)
    } else {
        let truncated = &msg[..max_len.saturating_sub(3)];
        format!("{}...", truncated)
    }
}

/// Formats a timestamp in nanoseconds as MM:SS.mmm
pub(crate) fn format_timestamp(timestamp_ns: u64) -> String {
    let total_secs = timestamp_ns / 1_000_000_000;
    let millis = (timestamp_ns % 1_000_000_000) / 1_000_000;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{:02}:{:02}.{:03}", minutes, seconds, millis)
}

/// Formats a time difference in nanoseconds as "now", "1s ago", "1m ago", "1h ago", etc.
pub(crate) fn format_time_ago(nanos_ago: u64) -> String {
    const NANOS_PER_SEC: u64 = 1_000_000_000;
    const NANOS_PER_MIN: u64 = 60 * NANOS_PER_SEC;
    const NANOS_PER_HOUR: u64 = 60 * NANOS_PER_MIN;

    if nanos_ago < NANOS_PER_SEC {
        "now".to_string()
    } else if nanos_ago < NANOS_PER_MIN {
        let secs = nanos_ago / NANOS_PER_SEC;
        if secs == 1 {
            "1s ago".to_string()
        } else {
            format!("{}s ago", secs)
        }
    } else if nanos_ago < NANOS_PER_HOUR {
        let mins = nanos_ago / NANOS_PER_MIN;
        if mins == 1 {
            "1m ago".to_string()
        } else {
            format!("{}m ago", mins)
        }
    } else {
        let hours = nanos_ago / NANOS_PER_HOUR;
        if hours == 1 {
            "1h ago".to_string()
        } else {
            format!("{}h ago", hours)
        }
    }
}
