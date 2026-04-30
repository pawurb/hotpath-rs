use std::collections::{HashMap, HashSet};

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn attribute_exclusive_traces(
    report: &pprof::Report,
    instrumented_names: &HashSet<&'static str>,
) -> HashMap<String, u64> {
    let mut attributed = HashMap::<String, u64>::new();

    for (stack, samples) in &report.data {
        let samples = match u64::try_from(*samples) {
            Ok(samples) if samples > 0 => samples,
            _ => continue,
        };

        let mut owner = None;
        for frame in &stack.frames {
            for sym in frame.iter().rev() {
                let symbol = format!("{sym}");
                let normalized = strip_rust_hash_suffix(&symbol);
                if instrumented_names.contains(normalized) {
                    owner = Some(normalized.to_string());
                    break;
                }
            }

            if owner.is_some() {
                break;
            }
        }

        if let Some(owner) = owner {
            *attributed.entry(owner).or_default() += samples;
        }
    }

    attributed
}

#[inline]
fn strip_rust_hash_suffix(symbol: &str) -> &str {
    let Some((prefix, suffix)) = symbol.rsplit_once("::h") else {
        return symbol;
    };

    if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_hexdigit()) {
        return symbol;
    }

    prefix
}

#[cfg(test)]
mod tests {
    use crate::lib_on::cpu::strip_rust_hash_suffix;

    #[test]
    fn strips_rust_hash_suffix() {
        assert_eq!(
            strip_rust_hash_suffix("profile_cpu::heavy_work::h1234abcd"),
            "profile_cpu::heavy_work"
        );
        assert_eq!(
            strip_rust_hash_suffix("profile_cpu::heavy_work"),
            "profile_cpu::heavy_work"
        );
        assert_eq!(
            strip_rust_hash_suffix("profile_cpu::heavy_work::handler"),
            "profile_cpu::heavy_work::handler"
        );
    }
}
