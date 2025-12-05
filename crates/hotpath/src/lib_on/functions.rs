use std::{collections::HashMap, time::Duration};

use crossbeam_channel::bounded;

use crate::{
    http_server::RECV_TIMEOUT_MS, FunctionLogsJson, FunctionsJson, FunctionsQuery, FUNCTIONS_STATE,
};

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc")] {
        pub mod alloc;
    } else {
        pub mod timing;
    }
}

pub mod guard;

pub(crate) fn get_function_logs_timing(function_name: &str) -> Option<FunctionLogsJson> {
    let arc_swap = FUNCTIONS_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();

    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<Option<FunctionLogsJson>>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx
            .send(FunctionsQuery::LogsTiming {
                function_name: function_name.to_string(),
                response_tx,
            })
            .ok()?;
        drop(state_guard);

        response_rx
            .recv_timeout(Duration::from_millis(RECV_TIMEOUT_MS))
            .ok()
            .flatten()
    } else {
        None
    }
}

pub(crate) fn get_functions_timing_json() -> FunctionsJson {
    if let Some(metrics) = try_get_functions_timing_from_worker() {
        return metrics;
    }

    // Fallback if query fails: return empty functions data
    FunctionsJson {
        hotpath_profiling_mode: crate::output::ProfilingMode::Timing,
        total_elapsed: 0,
        description: "No timing data available yet".to_string(),
        caller_name: "hotpath".to_string(),
        percentiles: vec![95],
        data: crate::output::FunctionsDataJson(HashMap::new()),
    }
}

fn try_get_functions_timing_from_worker() -> Option<FunctionsJson> {
    let arc_swap = FUNCTIONS_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();

    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<FunctionsJson>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx.send(FunctionsQuery::Timing(response_tx)).ok()?;
        drop(state_guard);

        response_rx
            .recv_timeout(Duration::from_millis(RECV_TIMEOUT_MS))
            .ok()
    } else {
        None
    }
}

// Will return None unless hotpath-alloc is enabled
pub(crate) fn get_function_logs_alloc(function_name: &str) -> Option<FunctionLogsJson> {
    let arc_swap = FUNCTIONS_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();

    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<Option<FunctionLogsJson>>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx
            .send(FunctionsQuery::LogsAlloc {
                function_name: function_name.to_string(),
                response_tx,
            })
            .ok()?;
        drop(state_guard);

        response_rx
            .recv_timeout(Duration::from_millis(RECV_TIMEOUT_MS))
            .ok()
            .flatten()
    } else {
        None
    }
}

pub(crate) fn get_functions_alloc_json() -> Option<FunctionsJson> {
    let arc_swap = FUNCTIONS_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();

    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<Option<FunctionsJson>>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx.send(FunctionsQuery::Alloc(response_tx)).ok()?;
        drop(state_guard);

        // Flatten the Option<Option<FunctionsJson>> to Option<FunctionsJson>
        response_rx
            .recv_timeout(Duration::from_millis(RECV_TIMEOUT_MS))
            .ok()
            .flatten()
    } else {
        None
    }
}
