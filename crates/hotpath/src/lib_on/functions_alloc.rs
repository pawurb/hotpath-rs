use std::time::Duration;

use crossbeam_channel::bounded;

use crate::{
    http_server::RECV_TIMEOUT_MS, FunctionLogsJson, FunctionsJson, QueryRequest, HOTPATH_STATE,
};

// Will return None unless hotpath-alloc is enabled
pub(crate) fn get_function_logs_alloc(function_name: &str) -> Option<FunctionLogsJson> {
    let arc_swap = HOTPATH_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();

    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<Option<FunctionLogsJson>>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx
            .send(QueryRequest::FunctionLogsAlloc {
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
    let arc_swap = HOTPATH_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();

    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<Option<FunctionsJson>>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx
            .send(QueryRequest::FunctionsAlloc(response_tx))
            .ok()?;
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
