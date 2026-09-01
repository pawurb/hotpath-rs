//! Format-neutral metric-family model for the Prometheus exporter. Collectors
//! build a `Vec<Family>` once from raw subsystem snapshots; the two encoders
//! (`to_text`, `to_protobuf`) walk it, so each family is defined in one place
//! regardless of scrape format. Histogram bounds and sums are stored already
//! unit-converted (seconds, bytes), keeping the encoders unit-agnostic.

use std::fmt::Write;

use crate::lib_on::native_histograms::to_spans;
use crate::prometheus_server::NATIVE_SCHEMA;

#[derive(Debug, Clone, Copy)]
pub(crate) enum FamilyKind {
    Counter,
    Gauge,
    Histogram,
}

impl FamilyKind {
    fn as_str(self) -> &'static str {
        match self {
            FamilyKind::Counter => "counter",
            FamilyKind::Gauge => "gauge",
            FamilyKind::Histogram => "histogram",
        }
    }
}

#[derive(Debug)]
pub(crate) struct HistogramValue {
    /// Observations the histogram actually saw (`_count`, the implicit `+Inf`
    /// bucket); under sampling this is the sampled count, not the call count.
    pub(crate) sample_count: u64,
    /// Sum over the same population as `sample_count`, in base units.
    pub(crate) sum: f64,
    /// `(upper bound in base units, cumulative count)` per classic bucket,
    /// ascending; `+Inf` is appended by the encoders, never stored.
    pub(crate) classic_buckets: Vec<(f64, u64)>,
    /// Sparse native buckets `(index, count)` at `NATIVE_SCHEMA`, ascending,
    /// zero-valued observations excluded.
    pub(crate) native_buckets: Vec<(i32, u64)>,
    /// Observations exactly at zero, exported via the native histogram's
    /// zero_count - zero has no finite log-scale bucket. Classic buckets
    /// include them cumulatively as usual. Only the alloc histograms record
    /// zeros today (non-allocating calls).
    pub(crate) zero_count: u64,
}

#[derive(Debug)]
pub(crate) enum SampleValue {
    Scalar(f64),
    Histogram(HistogramValue),
}

#[derive(Debug)]
pub(crate) struct Sample {
    /// Label pairs in emission order, values unescaped.
    pub(crate) labels: Vec<(&'static str, String)>,
    pub(crate) value: SampleValue,
}

#[derive(Debug)]
pub(crate) struct Family {
    pub(crate) name: &'static str,
    pub(crate) help: &'static str,
    pub(crate) kind: FamilyKind,
    pub(crate) samples: Vec<Sample>,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn to_text(families: &[Family]) -> String {
    let mut out = String::with_capacity(16 * 1024);
    for family in families {
        let _ = writeln!(out, "# HELP {} {}", family.name, family.help);
        let _ = writeln!(out, "# TYPE {} {}", family.name, family.kind.as_str());
        for sample in &family.samples {
            match &sample.value {
                SampleValue::Scalar(value) => {
                    let _ = writeln!(
                        out,
                        "{}{} {}",
                        family.name,
                        format_labels(&sample.labels),
                        value
                    );
                }
                SampleValue::Histogram(hist) => {
                    for &(upper_bound, cumulative) in &hist.classic_buckets {
                        let _ = writeln!(
                            out,
                            "{}_bucket{} {}",
                            family.name,
                            format_labels_with_le(&sample.labels, &upper_bound.to_string()),
                            cumulative
                        );
                    }
                    let _ = writeln!(
                        out,
                        "{}_bucket{} {}",
                        family.name,
                        format_labels_with_le(&sample.labels, "+Inf"),
                        hist.sample_count
                    );
                    let _ = writeln!(
                        out,
                        "{}_sum{} {}",
                        family.name,
                        format_labels(&sample.labels),
                        hist.sum
                    );
                    let _ = writeln!(
                        out,
                        "{}_count{} {}",
                        family.name,
                        format_labels(&sample.labels),
                        hist.sample_count
                    );
                }
            }
        }
    }
    out
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn to_protobuf(families: &[Family]) -> Option<Vec<u8>> {
    use crate::prometheus_server::protobuf::{
        label, Bucket, BucketSpan, Counter, Gauge, Histogram, Metric, MetricFamily, MetricType,
    };
    use prost::Message;

    let mut out = Vec::with_capacity(16 * 1024);
    for family in families {
        let metric_type = match family.kind {
            FamilyKind::Counter => MetricType::Counter,
            FamilyKind::Gauge => MetricType::Gauge,
            FamilyKind::Histogram => MetricType::Histogram,
        };
        let encoded = MetricFamily {
            name: Some(family.name.into()),
            help: Some(family.help.into()),
            r#type: Some(metric_type as i32),
            metric: family
                .samples
                .iter()
                .map(|sample| {
                    let mut metric = Metric {
                        label: sample
                            .labels
                            .iter()
                            .map(|(name, value)| label(name, value))
                            .collect(),
                        ..Default::default()
                    };
                    match &sample.value {
                        SampleValue::Scalar(value) => match family.kind {
                            FamilyKind::Counter => {
                                metric.counter = Some(Counter {
                                    value: Some(*value),
                                });
                            }
                            _ => {
                                metric.gauge = Some(Gauge {
                                    value: Some(*value),
                                });
                            }
                        },
                        SampleValue::Histogram(hist) => {
                            let (mut spans, deltas) = to_spans(&hist.native_buckets);
                            // A histogram with no buckets and zero_count 0 fails
                            // Prometheus's isNativeHistogram() check, and a native
                            // scrape then aborts on the nil histogram. The documented
                            // convention for empty native histograms is a single
                            // no-op span (offset 0, length 0).
                            if spans.is_empty() && hist.zero_count == 0 {
                                spans.push((0, 0));
                            }
                            metric.histogram = Some(Histogram {
                                sample_count: Some(hist.sample_count),
                                sample_sum: Some(hist.sum),
                                // +Inf is implicit in protobuf: the scraper
                                // derives it from sample_count.
                                bucket: hist
                                    .classic_buckets
                                    .iter()
                                    .map(|&(upper_bound, cumulative)| Bucket {
                                        cumulative_count: Some(cumulative),
                                        upper_bound: Some(upper_bound),
                                    })
                                    .collect(),
                                schema: Some(NATIVE_SCHEMA),
                                zero_threshold: Some(0.0),
                                zero_count: Some(hist.zero_count),
                                positive_span: spans
                                    .into_iter()
                                    .map(|(offset, length)| BucketSpan {
                                        offset: Some(offset),
                                        length: Some(length),
                                    })
                                    .collect(),
                                positive_delta: deltas,
                            });
                        }
                    }
                    metric
                })
                .collect(),
        };
        encoded.encode_length_delimited(&mut out).ok()?;
    }
    Some(out)
}

fn format_labels(labels: &[(&'static str, String)]) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let mut out = String::from("{");
    for (i, (name, value)) in labels.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{}=\"{}\"", name, escape_label_value(value));
    }
    out.push('}');
    out
}

fn format_labels_with_le(labels: &[(&'static str, String)], le: &str) -> String {
    let mut out = String::from("{");
    for (name, value) in labels {
        let _ = write!(out, "{}=\"{}\",", name, escape_label_value(value));
    }
    let _ = write!(out, "le=\"{}\"}}", le);
    out
}

fn escape_label_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            _ => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use crate::prometheus_server::families::{
        escape_label_value, to_protobuf, to_text, Family, FamilyKind, HistogramValue, Sample,
        SampleValue,
    };

    fn sample_families() -> Vec<Family> {
        vec![
            Family {
                name: "hotpath_up",
                help: "Test gauge.",
                kind: FamilyKind::Gauge,
                samples: vec![Sample {
                    labels: vec![],
                    value: SampleValue::Scalar(1.0),
                }],
            },
            Family {
                name: "hotpath_calls_total",
                help: "Test counter.",
                kind: FamilyKind::Counter,
                samples: vec![Sample {
                    labels: vec![("function", "app::run".to_string())],
                    value: SampleValue::Scalar(42.0),
                }],
            },
            Family {
                name: "hotpath_duration_seconds",
                help: "Test histogram.",
                kind: FamilyKind::Histogram,
                samples: vec![Sample {
                    labels: vec![("function", "app::run".to_string())],
                    value: SampleValue::Histogram(HistogramValue {
                        sample_count: 7,
                        sum: 0.5,
                        classic_buckets: vec![(0.00025, 2), (0.001, 5)],
                        native_buckets: vec![(-96, 2), (-95, 5)],
                        zero_count: 1,
                    }),
                }],
            },
        ]
    }

    #[test]
    fn text_encoding_matches_exposition_format() {
        let expected = "\
# HELP hotpath_up Test gauge.
# TYPE hotpath_up gauge
hotpath_up 1
# HELP hotpath_calls_total Test counter.
# TYPE hotpath_calls_total counter
hotpath_calls_total{function=\"app::run\"} 42
# HELP hotpath_duration_seconds Test histogram.
# TYPE hotpath_duration_seconds histogram
hotpath_duration_seconds_bucket{function=\"app::run\",le=\"0.00025\"} 2
hotpath_duration_seconds_bucket{function=\"app::run\",le=\"0.001\"} 5
hotpath_duration_seconds_bucket{function=\"app::run\",le=\"+Inf\"} 7
hotpath_duration_seconds_sum{function=\"app::run\"} 0.5
hotpath_duration_seconds_count{function=\"app::run\"} 7
";
        assert_eq!(to_text(&sample_families()), expected);
    }

    #[test]
    fn protobuf_encoding_round_trips() {
        use crate::prometheus_server::protobuf::{MetricFamily, MetricType};
        use prost::Message;

        let bytes = to_protobuf(&sample_families()).unwrap();
        let mut decoded = Vec::new();
        let mut buf = bytes.as_slice();
        while !buf.is_empty() {
            decoded.push(MetricFamily::decode_length_delimited(&mut buf).unwrap());
        }
        assert_eq!(decoded.len(), 3);

        assert_eq!(decoded[0].r#type, Some(MetricType::Gauge as i32));
        assert_eq!(
            decoded[0].metric[0].gauge.as_ref().unwrap().value,
            Some(1.0)
        );

        assert_eq!(decoded[1].r#type, Some(MetricType::Counter as i32));
        let counter_metric = &decoded[1].metric[0];
        assert_eq!(counter_metric.counter.as_ref().unwrap().value, Some(42.0));
        assert_eq!(counter_metric.label[0].value.as_deref(), Some("app::run"));

        assert_eq!(decoded[2].r#type, Some(MetricType::Histogram as i32));
        let hist = decoded[2].metric[0].histogram.as_ref().unwrap();
        assert_eq!(hist.sample_count, Some(7));
        assert_eq!(hist.sample_sum, Some(0.5));
        assert_eq!(hist.zero_count, Some(1));
        assert_eq!(hist.bucket.len(), 2);
        assert_eq!(hist.bucket[1].cumulative_count, Some(5));
        assert_eq!(hist.positive_span.len(), 1);
        assert_eq!(hist.positive_delta, vec![2, 3]);
    }

    #[test]
    fn empty_histogram_gets_noop_span() {
        use crate::prometheus_server::protobuf::MetricFamily;
        use prost::Message;

        let families = vec![Family {
            name: "hotpath_wait_seconds",
            help: "Test empty histogram.",
            kind: FamilyKind::Histogram,
            samples: vec![Sample {
                labels: vec![],
                value: SampleValue::Histogram(HistogramValue {
                    sample_count: 0,
                    sum: 0.0,
                    classic_buckets: vec![(0.00025, 0)],
                    native_buckets: vec![],
                    zero_count: 0,
                }),
            }],
        }];
        let bytes = to_protobuf(&families).unwrap();
        let decoded = MetricFamily::decode_length_delimited(&mut bytes.as_slice()).unwrap();
        let hist = decoded.metric[0].histogram.as_ref().unwrap();
        assert_eq!(hist.positive_span.len(), 1);
        assert_eq!(hist.positive_span[0].offset, Some(0));
        assert_eq!(hist.positive_span[0].length, Some(0));
        assert!(hist.positive_delta.is_empty());
    }

    #[test]
    fn escapes_label_values() {
        assert_eq!(escape_label_value(r#"a\b"c"#), r#"a\\b\"c"#);
        assert_eq!(escape_label_value("a\nb"), r"a\nb");
        assert_eq!(escape_label_value("plain"), "plain");
    }
}
