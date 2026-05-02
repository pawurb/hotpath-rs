use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

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

    let sidecar_path = sidecar_path_for(path);
    let sidecar = match load_sidecar(&sidecar_path) {
        Ok(s) => s,
        Err(e) => {
            log(
                "warn",
                format!(
                    "failed to load symbols sidecar {}: {e} - samply must be invoked with --unstable-presymbolicate",
                    sidecar_path.display()
                ),
            );
            return None;
        }
    };
    log(
        "info",
        format!(
            "sidecar loaded: {} libs, {} strings",
            sidecar.data.len(),
            sidecar.string_table.len()
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
    log(
        "info",
        format!("instrumented functions: {}", display_to_id.len()),
    );

    if display_to_id.is_empty() {
        log("warn", "no instrumented functions registered; CPU report empty");
        return None;
    }

    let eligible_symbols: HashSet<&'static str> = display_to_id.keys().copied().collect();

    let lib_indexes = build_lib_indexes(&profile, &sidecar, &eligible_symbols);
    let total_matches: usize = lib_indexes
        .iter()
        .filter_map(|i| i.as_ref().map(|x| x.ranges.len()))
        .sum();
    log(
        "info",
        format!("total matched symbols across libs: {total_matches}"),
    );
    if total_matches == 0 {
        log(
            "warn",
            "no instrumented symbols found in sidecar - check eligible names match samply's resolved names",
        );
    }

    let mut sample_counts: HashMap<&'static str, u64> = HashMap::new();
    let mut total_samples: u64 = 0;
    let mut attributed_samples: u64 = 0;
    let inclusive = *CPU_INCLUSIVE;
    let mut frames_seen: u64 = 0;

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
                let lib_opt = frame_func
                    .get(frame_idx)
                    .and_then(|fi| func_resource.get(*fi).copied())
                    .filter(|r| *r >= 0)
                    .and_then(|r| resource_lib.get(r as usize).copied().flatten())
                    .filter(|l| *l >= 0)
                    .map(|l| l as usize);

                frames_seen += 1;

                if address >= 0 {
                    if let Some(lib_idx) = lib_opt {
                        if let Some(Some(idx)) = lib_indexes.get(lib_idx) {
                            if let Some(sym) = idx.lookup(address as u64) {
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
            "samples: total={total_samples} attributed={attributed_samples} stats_rows={} frames_seen={frames_seen}",
            stats.len()
        ),
    );
    if attributed_samples == 0 {
        log(
            "warn",
            format!(
                "no samples were attributed to instrumented functions; total_samples={total_samples} matched_symbols={total_matches}"
            ),
        );
    } else if !stats.is_empty() {
        log(
            "info",
            format!(
                "top CPU function={} samples={} total_rows={}",
                stats[0].name, stats[0].samples, stats.len()
            ),
        );
    }

    Some(CpuReport {
        total_samples,
        attributed_samples,
        caller_name,
        stats,
    })
}

fn sidecar_path_for(profile_path: &Path) -> PathBuf {
    profile_path.with_extension("syms.json")
}

fn load_profile(path: &Path) -> Result<Profile, Box<dyn std::error::Error>> {
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

fn load_sidecar(path: &Path) -> Result<Sidecar, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice::<Sidecar>(&bytes)?)
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

fn build_lib_indexes(
    profile: &Profile,
    sidecar: &Sidecar,
    eligible: &HashSet<&'static str>,
) -> Vec<Option<LibSymbolIndex>> {
    let mut sidecar_by_name: HashMap<&str, &SidecarLib> = HashMap::new();
    for lib in &sidecar.data {
        sidecar_by_name.insert(lib.debug_name.as_str(), lib);
    }

    profile
        .libs
        .iter()
        .map(|lib| {
            let key = lib.debug_name.as_deref()?;
            let sl = sidecar_by_name.get(key)?;
            let mut ranges: Vec<(u64, u64, &'static str)> = sl
                .symbol_table
                .iter()
                .filter_map(|sym| {
                    let raw = sidecar.string_table.get(sym.symbol as usize)?;
                    let matched = match_eligible_symbol(raw, eligible)?;
                    let size = sym.size.unwrap_or(0) as u64;
                    let start = sym.rva as u64;
                    let end = if size > 0 { start + size } else { start + 1 };
                    Some((start, end, *matched))
                })
                .collect();
            ranges.sort_by_key(|(start, _, _)| *start);
            Some(LibSymbolIndex { ranges })
        })
        .collect()
}

fn match_eligible_symbol<'a>(
    resolved: &str,
    eligible: &'a HashSet<&'static str>,
) -> Option<&'a &'static str> {
    if let Some(exact) = eligible.get(resolved) {
        return Some(exact);
    }

    eligible
        .iter()
        .filter(|candidate| {
            resolved
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

#[derive(Deserialize)]
struct Lib {
    #[serde(rename = "debugName", default)]
    debug_name: Option<String>,
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

#[derive(Deserialize)]
struct Sidecar {
    #[serde(default)]
    data: Vec<SidecarLib>,
    #[serde(default)]
    string_table: Vec<String>,
}

#[derive(Deserialize)]
struct SidecarLib {
    debug_name: String,
    #[serde(default)]
    symbol_table: Vec<SidecarSymbol>,
}

#[derive(Deserialize)]
struct SidecarSymbol {
    rva: u32,
    #[serde(default)]
    size: Option<u32>,
    symbol: u32,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::lib_on::functions::cpu::samply::{match_eligible_symbol, sidecar_path_for};
    use std::path::Path;

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

    #[test]
    fn matches_exact_symbol() {
        let eligible = HashSet::from(["mevlog::main"]);
        let matched = match_eligible_symbol("mevlog::main", &eligible);
        assert_eq!(matched.copied(), Some("mevlog::main"));
    }

    #[test]
    fn sidecar_path_strips_gz() {
        assert_eq!(
            sidecar_path_for(Path::new("/tmp/hp.json.gz")),
            Path::new("/tmp/hp.json.syms.json")
        );
        assert_eq!(
            sidecar_path_for(Path::new("/tmp/hp.json")),
            Path::new("/tmp/hp.syms.json")
        );
    }
}
