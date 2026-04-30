use std::collections::{HashMap, HashSet};

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn report_cpu_attribution(
    pprof_guard: &pprof::ProfilerGuard<'static>,
    caller_name: &str,
) {
    let report = match pprof_guard.report().build() {
        Ok(report) => report,
        Err(e) => {
            eprintln!("[hotpath - pprof] failed to build report: {}", e);
            return;
        }
    };

    let total: isize = report.data.values().sum();
    eprintln!(
        "[hotpath - pprof] sampling report: {} unique stacks, {} total samples",
        report.data.len(),
        total
    );

    let mut stacks: Vec<_> = report.data.iter().collect();
    stacks.sort_by(|a, b| b.1.cmp(a.1));
    for (idx, (frames, count)) in stacks.iter().enumerate() {
        eprintln!(
            "\n[hotpath - pprof] stack #{} (samples: {}, thread: {} [{}])",
            idx + 1,
            count,
            frames.thread_name,
            frames.thread_id
        );
        for (depth, frame) in frames.frames.iter().enumerate() {
            for sym in frame {
                eprintln!("  #{:>2} {}", depth, sym);
            }
        }
    }

    let Some(instrumented_names) = crate::functions::get_instrumented_function_names() else {
        eprintln!(
            "[hotpath - cpu] failed to fetch registered measured function names before worker shutdown"
        );
        return;
    };

    let eligible_names: HashSet<&'static str> = instrumented_names
        .into_iter()
        .filter(|name| *name != caller_name)
        .collect();

    if eligible_names.is_empty() {
        eprintln!(
            "[hotpath - cpu] no eligible measured functions found after excluding wrapper {}",
            caller_name
        );
        return;
    }

    let attributed = attribute_exclusive_traces(&report, &eligible_names);
    if attributed.is_empty() {
        eprintln!("[hotpath - cpu] no sampled stacks matched eligible measured functions");
        return;
    }

    let mut attributed: Vec<_> = attributed.into_iter().collect();
    attributed.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    eprintln!("\n[hotpath - cpu] attributed traces:");
    for (name, count) in attributed {
        eprintln!("  {}: {}", name, count);
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn attribute_exclusive_traces(
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
            for sym in frame {
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
