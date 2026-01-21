//! MCP server output types - re-exports from formatted_output module.
//!
//! This module re-exports the formatted output types for MCP tool responses.
//! All formatting logic is centralized in the `formatted_output` module.

pub(crate) use crate::formatted_output::{
    FormattedFunctionAllocLogsJson as FunctionAllocLogsMCPJson,
    FormattedFunctionTimingLogsJson as FunctionTimingLogsMCPJson,
    FormattedFunctionsJson as FunctionsMCPJson,
};

#[cfg(test)]
mod tests {
    use crate::formatted_output::FormattedFunctionsJson;
    use crate::output::{FunctionsJson, MetricType, ProfilingMode};

    #[test]
    fn test_alloc_mode_formatting() {
        let raw = FunctionsJson {
            hotpath_profiling_mode: ProfilingMode::Alloc,
            total_elapsed: 1394730364208,
            description: "Cumulative allocations".to_string(),
            caller_name: "hotpath::main".to_string(),
            percentiles: vec![95],
            data: vec![(
                "render_ui".to_string(),
                vec![
                    MetricType::CallsCount(5178),
                    MetricType::Alloc(60437, 0),
                    MetricType::Alloc(60447, 0),
                    MetricType::Alloc(312947932, 0),
                    MetricType::Percentage(3884),
                ],
            )],
        };

        let formatted = FormattedFunctionsJson::from(&raw);

        assert_eq!(formatted.profiling_mode, "alloc");
        assert_eq!(formatted.total_elapsed, "1.3 TB");
        assert_eq!(formatted.data[0].calls, 5178);
        assert_eq!(formatted.data[0].avg, "59.0 KB");
        assert_eq!(formatted.data[0].percentiles.get("p95").unwrap(), "59.0 KB");
        assert_eq!(formatted.data[0].total, "298.5 MB");
        assert_eq!(formatted.data[0].percent_total, "38.84%");
    }
}
