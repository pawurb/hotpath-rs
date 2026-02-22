use arc_swap::ArcSwapOption;
use crossbeam_channel::{bounded, select_biased, unbounded};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::thread;
use std::time::Instant;

const DEFAULT_LOGS_LIMIT: usize = 50;
pub(crate) static LOGS_LIMIT: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("HOTPATH_META_LOGS_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_LOGS_LIMIT)
});

use std::io::Write;

use crate::json::{JsonCpuBaseline, JsonFunctionsList, JsonReport};
use crate::metrics_server::METRICS_SERVER_PORT;
use crate::output::{
    format_duration, resolve_output_path, FunctionLog, FunctionLogsList, MetricsProvider,
    OutputDestination,
};
use crate::output_on::{display_no_measurements_message_to, display_table_to, write_report_header};

use crate::functions::{FunctionsQuery, FUNCTIONS_QUERY_TX, FUNCTIONS_STATE};
use crate::lib_on::report;
use crate::shared::Section;

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc-meta")] {
        use crate::functions::alloc::{
            report::{StatsData, TimingStatsData},
            state::{FunctionStats, FunctionsState, Measurement, process_measurement, flush_batch},
        };
    } else {
        use crate::functions::timing::{
            report::StatsData,
            state::{FunctionStats, FunctionsState, Measurement, process_measurement, flush_batch},
        };
    }
}

use crate::functions::MeasurementGuard;
use crate::Format;

#[must_use = "builder is discarded without creating a guard"]
pub struct HotpathGuardBuilder {
    caller_name: &'static str,
    percentiles: Vec<u8>,
    format: Format,
    functions_limit: usize,
    channels_limit: usize,
    streams_limit: usize,
    futures_limit: usize,
    threads_limit: usize,
    output_path: Option<PathBuf>,
    sections: Option<Vec<Section>>,
    before_shutdown: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl HotpathGuardBuilder {
    pub fn new(caller_name: &'static str) -> Self {
        Self {
            caller_name,
            percentiles: vec![95],
            format: Format::Table,
            functions_limit: 10,
            channels_limit: 0,
            streams_limit: 0,
            futures_limit: 0,
            threads_limit: 5,
            output_path: None,
            sections: None,
            before_shutdown: None,
        }
    }

    pub fn percentiles(mut self, percentiles: &[u8]) -> Self {
        self.percentiles = percentiles.to_vec();
        self
    }

    pub fn with_functions_limit(mut self, limit: usize) -> Self {
        self.functions_limit = limit;
        self
    }

    pub fn with_channels_limit(mut self, limit: usize) -> Self {
        self.channels_limit = limit;
        self
    }

    pub fn with_streams_limit(mut self, limit: usize) -> Self {
        self.streams_limit = limit;
        self
    }

    pub fn with_futures_limit(mut self, limit: usize) -> Self {
        self.futures_limit = limit;
        self
    }

    pub fn with_threads_limit(mut self, limit: usize) -> Self {
        self.threads_limit = limit;
        self
    }

    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    pub fn output_path(mut self, path: impl AsRef<std::path::Path>) -> Self {
        self.output_path = Some(resolve_output_path(path));
        self
    }

    pub fn with_sections(mut self, sections: Vec<Section>) -> Self {
        self.sections = Some(sections);
        self
    }

    pub fn before_shutdown(mut self, f: impl FnOnce() + Send + Sync + 'static) -> Self {
        self.before_shutdown = Some(Box::new(f));
        self
    }

    fn resolve_sections(&self) -> Vec<Section> {
        if let Some(env_sections) = Section::from_env() {
            return env_sections;
        }

        if let Some(ref sections) = self.sections {
            return sections.clone();
        }

        cfg_if::cfg_if! {
            if #[cfg(feature = "hotpath-alloc-meta")] {
                vec![Section::FunctionsTiming, Section::FunctionsAlloc, Section::Threads]
            } else {
                vec![Section::FunctionsTiming, Section::Threads]
            }
        }
    }

    pub fn build(self) -> HotpathGuard {
        let sections = self.resolve_sections();

        HotpathGuard::new(
            self.caller_name,
            &self.percentiles,
            self.functions_limit,
            self.format,
            self.output_path,
            sections,
            self.before_shutdown,
            self.channels_limit,
            self.streams_limit,
            self.futures_limit,
            self.threads_limit,
        )
    }

    pub fn build_with_shutdown(self, duration: std::time::Duration) {
        let guard = self.build();
        if let Some(timeout) =
            crate::shared::resolve_timeout_duration(duration, "HOTPATH_META_SHUTDOWN_MS")
        {
            thread::spawn(move || {
                thread::sleep(timeout);
                drop(guard);
                std::process::exit(0);
            });
        } else {
            thread::spawn(move || {
                let _guard = guard;
                loop {
                    thread::park();
                }
            });
        }
    }
}

#[must_use = "guard is dropped immediately without generating a report"]
pub struct HotpathGuard {
    state: Arc<RwLock<FunctionsState>>,
    format: Format,
    wrapper_guard: Option<MeasurementGuard>,
    output_path: Option<PathBuf>,
    sections: Vec<Section>,
    start_time: Instant,
    before_shutdown: Option<Box<dyn FnOnce() + Send + Sync>>,
    channels_limit: usize,
    streams_limit: usize,
    futures_limit: usize,
    threads_limit: usize,
}

impl HotpathGuard {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        caller_name: &'static str,
        percentiles: &[u8],
        limit: usize,
        format: Format,
        output_path: Option<PathBuf>,
        sections: Vec<Section>,
        before_shutdown: Option<Box<dyn FnOnce() + Send + Sync>>,
        channels_limit: usize,
        streams_limit: usize,
        futures_limit: usize,
        threads_limit: usize,
    ) -> Self {
        #[cfg(feature = "hotpath-alloc-meta")]
        {
            crate::functions::alloc::core::ALLOCATIONS.with(|stack| {
                stack.tracking_enabled.set(false);
            });
            crate::functions::alloc::core::init_thread_alloc_tracking();
        }

        let percentiles = percentiles.to_vec();

        let arc_swap = FUNCTIONS_STATE.get_or_init(|| ArcSwapOption::from(None));

        if arc_swap.load().is_some() {
            panic!("More than one _hotpath guard cannot be alive at the same time.");
        }

        let (tx, rx) = unbounded::<Measurement>();
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<HashMap<u32, FunctionStats>>(1);
        let (query_tx, query_rx) = unbounded::<FunctionsQuery>();
        let _ = FUNCTIONS_QUERY_TX.set(query_tx);
        let start_time = Instant::now();

        let state_arc = Arc::new(RwLock::new(FunctionsState {
            sender: Some(tx),
            shutdown_tx: Some(shutdown_tx),
            completion_rx: Some(Mutex::new(completion_rx)),
            start_time,
            caller_name,
            percentiles: percentiles.clone(),
            limit,
        }));

        let worker_start_time = start_time;
        let worker_percentiles = percentiles.clone();
        let worker_caller_name = caller_name;
        let worker_limit = limit;
        thread::Builder::new()
            .name("hp-meta-functions".into())
            .spawn(move || {
                let mut local_stats = HashMap::<u32, FunctionStats>::new();
                let mut name_to_id = HashMap::<&'static str, u32>::new();

                loop {
                    select_biased! {
                        recv(shutdown_rx) -> _ => {
                            while let Ok(measurement) = rx.try_recv() {
                                process_measurement(&mut local_stats, &mut name_to_id, measurement, worker_start_time);
                            }
                            break;
                        }
                        recv(query_rx) -> result => {
                            if let Ok(query_request) = result {
                                match query_request {
                                    FunctionsQuery::Alloc(response_tx) => {
                                        cfg_if::cfg_if! {
                                            if #[cfg(feature = "hotpath-alloc-meta")] {
                                                let total_elapsed = worker_start_time.elapsed();
                                                let current_elapsed_ns = total_elapsed.as_nanos() as u64;
                                                let provider = StatsData::new(
                                                    &local_stats,
                                                    total_elapsed,
                                                    worker_percentiles.clone(),
                                                    worker_caller_name,
                                                    worker_limit,
                                                );
                                                let formatted = JsonFunctionsList::from_provider(&provider, current_elapsed_ns);
                                                let _ = response_tx.send(Some(formatted));
                                            } else {
                                                let _ = response_tx.send(None);
                                            }
                                        }
                                    }
                                    FunctionsQuery::Timing(response_tx) => {
                                        cfg_if::cfg_if! {
                                            if #[cfg(feature = "hotpath-alloc-meta")] {
                                                let total_elapsed = worker_start_time.elapsed();
                                                let current_elapsed_ns = total_elapsed.as_nanos() as u64;
                                                let provider = TimingStatsData::new(
                                                    &local_stats,
                                                    total_elapsed,
                                                    worker_percentiles.clone(),
                                                    worker_caller_name,
                                                    worker_limit,
                                                );
                                                let formatted = JsonFunctionsList::from_provider(&provider, current_elapsed_ns);
                                                let _ = response_tx.send(formatted);
                                            } else {
                                                let total_elapsed = worker_start_time.elapsed();
                                                let current_elapsed_ns = total_elapsed.as_nanos() as u64;
                                                let provider = StatsData::new(
                                                    &local_stats,
                                                    total_elapsed,
                                                    worker_percentiles.clone(),
                                                    worker_caller_name,
                                                    worker_limit,
                                                );
                                                let formatted = JsonFunctionsList::from_provider(&provider, current_elapsed_ns);
                                                let _ = response_tx.send(formatted);
                                            }
                                        }
                                    }
                                    FunctionsQuery::LogsTiming { function_id, response_tx } => {
                                        let response = local_stats.get(&function_id)
                                            .map(|stats| {
                                                cfg_if::cfg_if! {
                                                    if #[cfg(feature = "hotpath-alloc-meta")] {
                                                        let logs: Vec<FunctionLog> = stats.recent_logs
                                                            .iter()
                                                            .rev()
                                                            .map(|(_bytes, _count, duration_ns, elapsed, tid, result_log)| FunctionLog {
                                                                value: Some(*duration_ns),
                                                                elapsed_nanos: elapsed.as_nanos() as u64,
                                                                alloc_count: None,
                                                                tid: *tid,
                                                                result: result_log.clone(),
                                                            })
                                                            .collect();
                                                    } else {
                                                        let logs: Vec<FunctionLog> = stats.recent_logs
                                                            .iter()
                                                            .rev()
                                                            .map(|(duration_ns, elapsed, tid, result_log)| FunctionLog {
                                                                value: Some(*duration_ns),
                                                                elapsed_nanos: elapsed.as_nanos() as u64,
                                                                alloc_count: None,
                                                                tid: *tid,
                                                                result: result_log.clone(),
                                                            })
                                                            .collect();
                                                    }
                                                }
                                                FunctionLogsList {
                                                    function_name: stats.name.to_string(),
                                                    logs,
                                                    count: stats.count as usize,
                                                }
                                            });
                                        let _ = response_tx.send(response);
                                    }
                                    FunctionsQuery::LogsAlloc { function_id, response_tx } => {
                                        cfg_if::cfg_if! {
                                            if #[cfg(feature = "hotpath-alloc-meta")] {
                                                let response = local_stats.get(&function_id)
                                                    .map(|stats| {
                                                        let logs: Vec<FunctionLog> = stats.recent_logs
                                                            .iter()
                                                            .rev()
                                                            .map(|(bytes, count, _duration_ns, elapsed, tid, result_log)| FunctionLog {
                                                                value: *bytes,
                                                                elapsed_nanos: elapsed.as_nanos() as u64,
                                                                alloc_count: *count,
                                                                tid: *tid,
                                                                result: result_log.clone(),
                                                            })
                                                            .collect();
                                                        FunctionLogsList {
                                                            function_name: stats.name.to_string(),
                                                            logs,
                                                            count: stats.count as usize,
                                                        }
                                                    });
                                                let _ = response_tx.send(response);
                                            } else {
                                                let _ = function_id;
                                                let _ = response_tx.send(None);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        recv(rx) -> result => {
                            match result {
                                Ok(measurement) => {
                                    process_measurement(&mut local_stats, &mut name_to_id, measurement, worker_start_time);
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }

                let _ = completion_tx.send(local_stats);
            })
            .expect("Failed to spawn hotpath-meta-worker thread");

        arc_swap.store(Some(Arc::clone(&state_arc)));

        #[cfg(target_os = "linux")]
        crate::lib_on::START_TIME.get_or_init(quanta::Instant::now);
        #[cfg(not(target_os = "linux"))]
        crate::lib_on::START_TIME.get_or_init(std::time::Instant::now);

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        #[cfg(feature = "hotpath-mcp-meta")]
        crate::mcp_server::start_mcp_server_once();

        if sections.contains(&Section::Futures) {
            crate::futures::init_futures_state();
        }

        #[cfg(feature = "threads")]
        if sections.contains(&Section::Threads) {
            crate::threads::init_threads_monitoring();
            #[cfg(feature = "hotpath-alloc-meta")]
            crate::functions::alloc::core::init_thread_alloc_tracking();
        }

        crate::cpu_baseline::init_cpu_baseline();

        let wrapper_guard = MeasurementGuard::build(caller_name, true, false);

        #[cfg(feature = "hotpath-alloc-meta")]
        crate::functions::alloc::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(true);
        });

        Self {
            state: Arc::clone(&state_arc),
            format,
            wrapper_guard: Some(wrapper_guard),
            output_path,
            sections,
            start_time,
            before_shutdown,
            channels_limit,
            streams_limit,
            futures_limit,
            threads_limit,
        }
    }
}

fn apply_limit(len: usize, limit: usize) -> usize {
    if limit > 0 && limit < len {
        limit
    } else {
        len
    }
}

impl Drop for HotpathGuard {
    fn drop(&mut self) {
        if let Some(f) = self.before_shutdown.take() {
            f();
        }

        let wrapper_guard = self.wrapper_guard.take().unwrap();
        drop(wrapper_guard);

        flush_batch();

        let cpu_baseline = crate::cpu_baseline::shutdown_cpu_baseline();

        let state: Arc<RwLock<FunctionsState>> = Arc::clone(&self.state);
        let elapsed = self.start_time.elapsed();

        let (shutdown_tx, completion_rx, end_time) = {
            let Ok(mut state_guard) = state.write() else {
                return;
            };

            state_guard.sender = None;
            let end_time = Instant::now();

            let shutdown_tx = state_guard.shutdown_tx.take();
            let completion_rx = state_guard.completion_rx.take();
            (shutdown_tx, completion_rx, end_time)
        };

        if let Some(tx) = shutdown_tx {
            let _ = tx.send(());
        }

        let functions_stats =
            completion_rx.and_then(|rx_mutex| rx_mutex.lock().ok().and_then(|rx| rx.recv().ok()));

        let channels_data = if self.sections.contains(&Section::Channels) {
            report::shutdown_channels()
        } else {
            Vec::new()
        };

        let streams_data = if self.sections.contains(&Section::Streams) {
            report::shutdown_streams()
        } else {
            Vec::new()
        };

        let futures_data = if self.sections.contains(&Section::Futures) {
            report::shutdown_futures()
        } else {
            Vec::new()
        };

        let output = OutputDestination::from_path(self.output_path.take());
        crate::output::set_use_colors(
            matches!(output, OutputDestination::Stdout) && std::env::var("NO_COLOR").is_err(),
        );
        let format = if std::env::var("HOTPATH_META_OUTPUT_FORMAT").is_ok() {
            Format::from_env()
        } else {
            self.format
        };

        let mut writer = match output.writer() {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create output writer: {}", e);
                return;
            }
        };

        let is_json = matches!(format, Format::Json | Format::JsonPretty);

        if is_json {
            let mut report = JsonReport {
                label: std::env::var("HOTPATH_META_REPORT_LABEL")
                    .ok()
                    .filter(|s| !s.is_empty()),
                ..Default::default()
            };

            for section in &self.sections {
                match section {
                    Section::FunctionsTiming => {
                        if let Some(ref stats) = functions_stats {
                            if let Ok(state_guard) = state.read() {
                                let total_elapsed = end_time.duration_since(state_guard.start_time);
                                let elapsed_ns = total_elapsed.as_nanos() as u64;

                                cfg_if::cfg_if! {
                                    if #[cfg(feature = "hotpath-alloc-meta")] {
                                        let provider = TimingStatsData::new(
                                            stats,
                                            total_elapsed,
                                            state_guard.percentiles.clone(),
                                            state_guard.caller_name,
                                            state_guard.limit,
                                        );
                                    } else {
                                        let provider = StatsData::new(
                                            stats,
                                            total_elapsed,
                                            state_guard.percentiles.clone(),
                                            state_guard.caller_name,
                                            state_guard.limit,
                                        );
                                    }
                                }

                                report.functions_timing =
                                    Some(JsonFunctionsList::from_provider(&provider, elapsed_ns));
                            }
                        }
                    }
                    Section::FunctionsAlloc => {
                        cfg_if::cfg_if! {
                            if #[cfg(feature = "hotpath-alloc-meta")] {
                                if let Some(ref stats) = functions_stats {
                                    if let Ok(state_guard) = state.read() {
                                        let total_elapsed = end_time.duration_since(state_guard.start_time);
                                        let elapsed_ns = total_elapsed.as_nanos() as u64;
                                        let provider = StatsData::new(
                                            stats,
                                            total_elapsed,
                                            state_guard.percentiles.clone(),
                                            state_guard.caller_name,
                                            state_guard.limit,
                                        );
                                        report.functions_alloc = Some(
                                            JsonFunctionsList::from_provider(&provider, elapsed_ns),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Section::Channels => {
                        if !channels_data.is_empty() {
                            let limit = apply_limit(channels_data.len(), self.channels_limit);
                            report.channels = Some(report::collect_channels_json(
                                &channels_data[..limit],
                                elapsed,
                            ));
                        }
                    }
                    Section::Streams => {
                        if !streams_data.is_empty() {
                            let limit = apply_limit(streams_data.len(), self.streams_limit);
                            report.streams = Some(report::collect_streams_json(
                                &streams_data[..limit],
                                elapsed,
                            ));
                        }
                    }
                    Section::Futures => {
                        if !futures_data.is_empty() {
                            let limit = apply_limit(futures_data.len(), self.futures_limit);
                            report.futures = Some(report::collect_futures_json(
                                &futures_data[..limit],
                                elapsed,
                            ));
                        }
                    }
                    Section::Threads => {
                        #[cfg(feature = "threads")]
                        {
                            let json = report::collect_threads_json(self.threads_limit);
                            if !json.data.is_empty() {
                                report.threads = Some(json);
                            }
                        }
                    }
                }
            }

            if let Some(ref baseline) = cpu_baseline {
                report.cpu_baseline = Some(JsonCpuBaseline {
                    avg: format_duration(baseline.avg_ns),
                });
            }

            match format {
                Format::Json => {
                    let _ = writeln!(
                        writer,
                        "{}",
                        serde_json::to_string(&report).unwrap_or_default()
                    );
                }
                Format::JsonPretty => {
                    let _ = writeln!(
                        writer,
                        "{}",
                        serde_json::to_string_pretty(&report).unwrap_or_default()
                    );
                }
                _ => {}
            }
        } else {
            let baseline_ns = cpu_baseline.as_ref().map(|b| b.avg_ns);
            let label = std::env::var("HOTPATH_META_REPORT_LABEL")
                .ok()
                .filter(|s| !s.is_empty());
            if matches!(format, Format::Table) {
                write_report_header(
                    &mut writer,
                    elapsed,
                    &self.sections,
                    baseline_ns,
                    label.as_deref(),
                );
            }

            for section in &self.sections {
                match section {
                    Section::FunctionsTiming => {
                        if let Some(ref stats) = functions_stats {
                            if let Ok(state_guard) = state.read() {
                                let total_elapsed = end_time.duration_since(state_guard.start_time);

                                cfg_if::cfg_if! {
                                    if #[cfg(feature = "hotpath-alloc-meta")] {
                                        let provider = TimingStatsData::new(
                                            stats,
                                            total_elapsed,
                                            state_guard.percentiles.clone(),
                                            state_guard.caller_name,
                                            state_guard.limit,
                                        );
                                    } else {
                                        let provider = StatsData::new(
                                            stats,
                                            total_elapsed,
                                            state_guard.percentiles.clone(),
                                            state_guard.caller_name,
                                            state_guard.limit,
                                        );
                                    }
                                }

                                match format {
                                    Format::Table => {
                                        if provider.metric_data().is_empty() {
                                            display_no_measurements_message_to(
                                                &mut writer,
                                                total_elapsed,
                                                state_guard.caller_name,
                                            );
                                        } else {
                                            display_table_to(&mut writer, &provider);
                                        }
                                    }
                                    Format::None => {}
                                    _ => {}
                                }
                            }
                        }
                    }
                    Section::FunctionsAlloc => {
                        cfg_if::cfg_if! {
                            if #[cfg(feature = "hotpath-alloc-meta")] {
                                if let Some(ref stats) = functions_stats {
                                    if let Ok(state_guard) = state.read() {
                                        let total_elapsed = end_time.duration_since(state_guard.start_time);
                                        let provider = StatsData::new(
                                            stats,
                                            total_elapsed,
                                            state_guard.percentiles.clone(),
                                            state_guard.caller_name,
                                            state_guard.limit,
                                        );

                                        match format {
                                            Format::Table => {
                                                if provider.metric_data().is_empty() {
                                                    display_no_measurements_message_to(
                                                        &mut writer,
                                                        total_elapsed,
                                                        state_guard.caller_name,
                                                    );
                                                } else {
                                                    display_table_to(&mut writer, &provider);
                                                }
                                            }
                                            Format::None => {}
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Section::Channels => {
                        if matches!(format, Format::Table) {
                            let total = channels_data.len();
                            let limit = apply_limit(total, self.channels_limit);
                            report::report_channels_table(
                                &channels_data[..limit],
                                total,
                                &mut writer,
                            );
                        }
                    }
                    Section::Streams => {
                        if matches!(format, Format::Table) {
                            let total = streams_data.len();
                            let limit = apply_limit(total, self.streams_limit);
                            report::report_streams_table(
                                &streams_data[..limit],
                                total,
                                &mut writer,
                            );
                        }
                    }
                    Section::Futures => {
                        if matches!(format, Format::Table) {
                            let total = futures_data.len();
                            let limit = apply_limit(total, self.futures_limit);
                            report::report_futures_table(
                                &futures_data[..limit],
                                total,
                                &mut writer,
                            );
                        }
                    }
                    Section::Threads =>
                    {
                        #[cfg(feature = "threads")]
                        if matches!(format, Format::Table) {
                            report::report_threads_table(&mut writer, self.threads_limit);
                        }
                    }
                }
            }
        }

        if let Some(arc_swap) = FUNCTIONS_STATE.get() {
            arc_swap.store(None);
        }
    }
}
