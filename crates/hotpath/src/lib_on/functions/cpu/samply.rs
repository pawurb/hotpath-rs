use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use object::{Object, ObjectSegment, ObjectSymbol, SymbolKind};
use serde::Deserialize;

use crate::lib_on::functions::cpu::{log, CpuFunctionStats, CpuReport, CPU_INCLUSIVE};

pub(crate) fn build_cpu_report_from_path(
    caller_name: &'static str,
    path: &Path,
) -> Option<CpuReport> {
    log("info", format!("loading samply profile from {}", path.display()));

    let profile = match load_profile(path) {
        Ok(p) => p,
        Err(e) => {
            log(
                "warn",
                format!("failed to load samply profile {}: {e}", path.display()),
            );
            return None;
        }
    };
    log(
        "info",
        format!(
            "profile loaded: {} libs, {} threads",
            profile.libs.len(),
            profile.threads.len()
        ),
    );

    let display_to_id = match crate::lib_on::functions::get_instrumented_names_and_ids() {
        Some(display_to_id) => display_to_id,
        None => {
            log(
                "warn",
                "instrumented function registry unavailable; skipping CPU report",
            );
            return None;
        }
    };
    log("info", format!("instrumented functions: {}", display_to_id.len()));

    if display_to_id.is_empty() {
        log("warn", "no instrumented functions registered; CPU report empty");
        return None;
    }

    let eligible_symbols: HashSet<&'static str> = display_to_id.keys().copied().collect();

    let lib_indexes: Vec<LibSymbolIndex> = profile
        .libs
        .iter()
        .enumerate()
        .map(|(i, lib)| {
            let lib_path = lib
                .debug_path
                .as_deref()
                .filter(|s| !s.is_empty())
                .or(lib.path.as_deref())
                .unwrap_or("<missing>");
            let idx = build_lib_index(lib, &eligible_symbols).unwrap_or_default();
            log(
                "info",
                format!("lib[{i}] {lib_path}: {} matching symbols", idx.ranges.len()),
            );
            if !idx.ranges.is_empty() {
                let first = idx.ranges.first().unwrap();
                let last = idx.ranges.last().unwrap();
                log(
                    "info",
                    format!(
                        "lib[{i}] range bounds: 0x{:x}..0x{:x} (first sym {:?}, last sym {:?})",
                        first.0, last.1, first.2, last.2
                    ),
                );
            }
            idx
        })
        .collect();
    let total_matches: usize = lib_indexes.iter().map(|i| i.ranges.len()).sum();
    log(
        "info",
        format!("total matched symbols across libs: {total_matches}"),
    );
    if total_matches == 0 {
        log(
            "warn",
            "no instrumented symbols found in any sampled library - ensure the binary was built with debug symbols and not stripped",
        );
    }

    let mut sample_counts: HashMap<&'static str, u64> = HashMap::new();
    let mut total_samples: u64 = 0;
    let mut attributed_samples: u64 = 0;
    let inclusive = *CPU_INCLUSIVE;
    let mut frames_seen: u64 = 0;
    let mut frames_with_lib: HashMap<usize, u64> = HashMap::new();
    let mut frames_no_lib: u64 = 0;
    let mut frames_no_addr: u64 = 0;
    let mut sample_addrs: Vec<(usize, i64)> = Vec::new();
    let mut lookup_logs: Vec<(usize, u64, Option<&'static str>)> = Vec::new();

    for thread in &profile.threads {
        let stack = &thread.samples.stack;
        let thread_cpu_deltas = thread.samples.thread_cpu_delta.as_ref();
        let weights = thread.samples.weight.as_ref();
        let prefix = &thread.stack_table.prefix;
        let stack_frame = &thread.stack_table.frame;
        let frame_addr = &thread.frame_table.address;
        let frame_func = &thread.frame_table.func;
        let func_resource = &thread.func_table.resource;
        let resource_lib = &thread.resource_table.lib;

        for (i, root) in stack.iter().enumerate() {
            let weight = sample_cpu_weight(thread_cpu_deltas, weights, i);
            total_samples += weight;

            let mut matched: HashSet<&'static str> = HashSet::new();
            let mut credited = false;
            let mut cur = *root;
            while let Some(s) = cur {
                let frame_idx = match stack_frame.get(s) {
                    Some(f) => *f,
                    None => break,
                };
                let address = frame_addr.get(frame_idx).copied().unwrap_or(-1);
                let func_idx = frame_func.get(frame_idx).copied();
                let lib_opt = func_idx
                    .and_then(|fi| func_resource.get(fi).copied())
                    .filter(|r| *r >= 0)
                    .and_then(|r| resource_lib.get(r as usize).copied().flatten())
                    .filter(|l| *l >= 0)
                    .map(|l| l as usize);

                frames_seen += 1;
                if address < 0 {
                    frames_no_addr += 1;
                }
                match lib_opt {
                    Some(li) => {
                        *frames_with_lib.entry(li).or_insert(0) += 1;
                        if address >= 0 && sample_addrs.len() < 20 {
                            sample_addrs.push((li, address));
                        }
                    }
                    None => frames_no_lib += 1,
                }

                if address >= 0 {
                    if let Some(lib_idx) = lib_opt {
                        if let Some(idx) = lib_indexes.get(lib_idx) {
                            let result = idx.lookup(address as u64);
                            if !idx.ranges.is_empty() && lookup_logs.len() < 30 {
                                lookup_logs.push((lib_idx, address as u64, result));
                            }
                            if let Some(sym) = result {
                                if inclusive {
                                    matched.insert(sym);
                                } else {
                                    *sample_counts.entry(sym).or_insert(0) += weight;
                                    credited = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                cur = prefix.get(s).copied().flatten();
            }

            if inclusive && !matched.is_empty() {
                for sym in &matched {
                    *sample_counts.entry(*sym).or_insert(0) += weight;
                }
                attributed_samples += weight;
            } else if credited {
                attributed_samples += weight;
            }
        }
    }

    let mut stats: Vec<CpuFunctionStats> = sample_counts
        .into_iter()
        .filter_map(|(name, samples)| {
            display_to_id.get(name).map(|id| CpuFunctionStats {
                name,
                id: *id,
                samples,
            })
        })
        .collect();

    stats.sort_by(|a, b| b.samples.cmp(&a.samples).then_with(|| a.name.cmp(b.name)));

    log(
        "info",
        format!(
            "samples: total={total_samples} attributed={attributed_samples} stats_rows={}",
            stats.len()
        ),
    );
    log(
        "info",
        format!("frames: seen={frames_seen} no_addr={frames_no_addr} no_lib={frames_no_lib}"),
    );
    if attributed_samples == 0 {
        log(
            "warn",
            format!(
                "no samples were attributed to instrumented functions; total_samples={} matched_symbols={} frames_seen={}",
                total_samples, total_matches, frames_seen
            ),
        );
    } else if stats.is_empty() {
        log(
            "warn",
            format!(
                "CPU profile parsed but produced no stats rows; total_samples={} attributed_samples={} matched_symbols={}",
                total_samples, attributed_samples, total_matches
            ),
        );
    } else {
        log(
            "info",
            format!(
                "top CPU function={} samples={} total_rows={}",
                stats[0].name,
                stats[0].samples,
                stats.len()
            ),
        );
    }
    let mut by_lib: Vec<(usize, u64)> = frames_with_lib.into_iter().collect();
    by_lib.sort_by_key(|(_, c)| std::cmp::Reverse(*c));

    Some(CpuReport {
        total_samples,
        attributed_samples,
        caller_name,
        stats,
    })
}

fn load_profile(path: &std::path::Path) -> Result<Profile, Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    let bytes: Vec<u8> = if buf.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = flate2::read::GzDecoder::new(&buf[..]);
        let mut decoded = Vec::new();
        decoder.read_to_end(&mut decoded)?;
        decoded
    } else {
        buf
    };

    Ok(serde_json::from_slice::<Profile>(&bytes)?)
}

#[derive(Default)]
struct LibSymbolIndex {
    ranges: Vec<(u64, u64, &'static str)>,
}

impl LibSymbolIndex {
    fn lookup(&self, addr: u64) -> Option<&'static str> {
        if self.ranges.is_empty() {
            return None;
        }
        let idx = self.ranges.partition_point(|(start, _, _)| *start <= addr);
        if idx == 0 {
            return None;
        }
        let (start, end, sym) = self.ranges[idx - 1];
        if addr >= start && addr < end {
            Some(sym)
        } else {
            None
        }
    }
}

fn build_lib_index(lib: &Lib, eligible: &HashSet<&'static str>) -> Option<LibSymbolIndex> {
    let path = lib
        .debug_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(lib.path.as_deref())?;

    let bytes = std::fs::read(path).ok()?;
    let parsed = object::File::parse(bytes.as_slice()).ok()?;

    let base = pick_image_base(&parsed);

    let mut all_starts: Vec<u64> = parsed
        .symbols()
        .filter(|s| matches!(s.kind(), SymbolKind::Text))
        .map(|s| s.address().saturating_sub(base))
        .collect();
    all_starts.sort_unstable();
    all_starts.dedup();

    let mut ranges: Vec<(u64, u64, &'static str)> = Vec::new();
    for sym in parsed.symbols() {
        if !matches!(sym.kind(), SymbolKind::Text) {
            continue;
        }
        let raw_name = match sym.name() {
            Ok(n) if !n.is_empty() => n,
            _ => continue,
        };
        let demangled = rustc_demangle::demangle(raw_name).to_string();
        let normalized = strip_hash_suffix(&demangled);
        if let Some(matched) = match_eligible_symbol(normalized, eligible) {
            let rel = sym.address().saturating_sub(base);
            let declared = sym.size();
            let next = all_starts
                .partition_point(|s| *s <= rel)
                .checked_sub(0)
                .and_then(|i| all_starts.get(i).copied());
            let size = if declared > 0 {
                declared
            } else {
                next.map(|n| n.saturating_sub(rel))
                    .filter(|s| *s > 0)
                    .unwrap_or(1)
            };
            ranges.push((rel, rel.saturating_add(size), *matched));
        }
    }

    ranges.sort_by_key(|(start, _, _)| *start);

    Some(LibSymbolIndex { ranges })
}

fn pick_image_base<'a>(file: &object::File<'a, &'a [u8]>) -> u64 {
    let rel = file.relative_address_base();
    if rel != 0 {
        return rel;
    }
    file.segments()
        .filter_map(|seg| {
            let name = seg.name().ok().flatten()?;
            if name == "__TEXT" || name == "__text" {
                Some(seg.address())
            } else {
                None
            }
        })
        .next()
        .or_else(|| file.segments().map(|s| s.address()).min())
        .unwrap_or(0)
}

fn strip_hash_suffix(s: &str) -> &str {
    if let Some(idx) = s.rfind("::h") {
        let suffix = &s[idx + 3..];
        if !suffix.is_empty() && suffix.len() <= 16 && suffix.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return &s[..idx];
        }
    }
    s
}

fn match_eligible_symbol<'a>(
    normalized: &str,
    eligible: &'a HashSet<&'static str>,
) -> Option<&'a &'static str> {
    if let Some(exact) = eligible.get(normalized) {
        return Some(exact);
    }

    eligible
        .iter()
        .filter(|candidate| {
            normalized
                .strip_prefix(**candidate)
                .is_some_and(|rest| rest.starts_with("::"))
        })
        .max_by_key(|candidate| candidate.len())
}

#[derive(Deserialize)]
struct Profile {
    #[serde(default)]
    libs: Vec<Lib>,
    #[serde(default)]
    threads: Vec<Thread>,
}

#[derive(Deserialize, Default)]
struct Lib {
    #[serde(default)]
    path: Option<String>,
    #[serde(rename = "debugPath", default)]
    debug_path: Option<String>,
}

#[derive(Deserialize)]
struct Thread {
    samples: Samples,
    #[serde(rename = "stackTable")]
    stack_table: StackTable,
    #[serde(rename = "frameTable")]
    frame_table: FrameTable,
    #[serde(rename = "funcTable")]
    func_table: FuncTable,
    #[serde(rename = "resourceTable")]
    resource_table: ResourceTable,
}

#[derive(Deserialize)]
struct Samples {
    #[serde(default)]
    stack: Vec<Option<usize>>,
    #[serde(default)]
    weight: Option<Vec<i64>>,
    #[serde(rename = "threadCPUDelta", default)]
    thread_cpu_delta: Option<Vec<i64>>,
}

fn sample_cpu_weight(
    thread_cpu_deltas: Option<&Vec<i64>>,
    weights: Option<&Vec<i64>>,
    index: usize,
) -> u64 {
    if let Some(delta) = thread_cpu_deltas.and_then(|deltas| deltas.get(index).copied()) {
        return delta.max(0) as u64;
    }

    weights
        .and_then(|weight_values| weight_values.get(index).copied())
        .map(|weight| weight.max(0) as u64)
        .unwrap_or(1)
}

#[derive(Deserialize)]
struct StackTable {
    #[serde(default)]
    prefix: Vec<Option<usize>>,
    #[serde(default)]
    frame: Vec<usize>,
}

#[derive(Deserialize)]
struct FrameTable {
    #[serde(default)]
    address: Vec<i64>,
    #[serde(default)]
    func: Vec<usize>,
}

#[derive(Deserialize)]
struct FuncTable {
    #[serde(default)]
    resource: Vec<i64>,
}

#[derive(Deserialize)]
struct ResourceTable {
    #[serde(default)]
    lib: Vec<Option<i64>>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::lib_on::functions::cpu::samply::{match_eligible_symbol, strip_hash_suffix};

    #[test]
    fn strips_rust_hash_suffix() {
        assert_eq!(
            strip_hash_suffix("mevlog::main::h4096f6e9269ba5f4"),
            "mevlog::main"
        );
        assert_eq!(strip_hash_suffix("mevlog::main"), "mevlog::main");
    }

    #[test]
    fn matches_async_closure_symbol_by_prefix() {
        let eligible = HashSet::from([
            "mevlog::misc::rpc_tracing::rpc_tx_calls",
            "mevlog::misc::shared_init::init_deps",
        ]);

        let matched = match_eligible_symbol(
            "mevlog::misc::rpc_tracing::rpc_tx_calls::{{closure}}::{{closure}}",
            &eligible,
        );

        assert_eq!(
            matched.copied(),
            Some("mevlog::misc::rpc_tracing::rpc_tx_calls")
        );
    }

    #[test]
    fn prefers_longest_prefix_match() {
        let eligible = HashSet::from(["mevlog::misc", "mevlog::misc::rpc_tracing::rpc_tx_calls"]);

        let matched = match_eligible_symbol(
            "mevlog::misc::rpc_tracing::rpc_tx_calls::{{closure}}",
            &eligible,
        );

        assert_eq!(
            matched.copied(),
            Some("mevlog::misc::rpc_tracing::rpc_tx_calls")
        );
    }
}
