use hotpath::{ceil_char_boundary, floor_char_boundary};

pub(crate) fn truncate_left(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let truncated_len = max_len.saturating_sub(3);
        let start_idx = ceil_char_boundary(s, s.len().saturating_sub(truncated_len));
        format!("...{}", &s[start_idx..])
    }
}

pub(crate) fn truncate_right(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let end = floor_char_boundary(s, max_len.saturating_sub(3));
        format!("{}...", &s[..end])
    }
}

pub(crate) fn truncate_message(msg: &str, max_len: usize) -> String {
    if msg.len() <= max_len {
        format!("{:<width$}", msg, width = max_len)
    } else {
        let end = floor_char_boundary(msg, max_len.saturating_sub(3));
        format!("{}...", &msg[..end])
    }
}
