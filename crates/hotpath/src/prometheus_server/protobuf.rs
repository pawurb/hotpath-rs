//! Hand-declared mirror of `io.prometheus.client` (prometheus/client_model
//! metrics.proto), restricted to the fields hotpath emits. The tag numbers are
//! the wire contract - keep them exactly in sync with upstream:
//!
//! ```text
//! MetricFamily: name=1(string) help=2(string) type=3(enum) metric=4(repeated Metric)
//! Metric:       label=1(repeated LabelPair) gauge=2 counter=3 histogram=7
//! LabelPair:    name=1(string) value=2(string)
//! Counter:      value=1(double)
//! Gauge:        value=1(double)
//! Histogram:    sample_count=1(uint64) sample_sum=2(double) bucket=3(repeated Bucket)
//!               schema=5(sint32) zero_threshold=6(double) zero_count=7(uint64)
//!               positive_span=12(repeated BucketSpan) positive_delta=13(repeated sint64)
//! Bucket:       cumulative_count=1(uint64) upper_bound=2(double)
//! BucketSpan:   offset=1(sint32) length=2(uint32)
//! ```
//!
//! metrics.proto is proto2, so repeated scalars are unpacked (`packed = "false"`).

#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub(crate) enum MetricType {
    Counter = 0,
    Gauge = 1,
    Summary = 2,
    Untyped = 3,
    Histogram = 4,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct MetricFamily {
    #[prost(string, optional, tag = "1")]
    pub(crate) name: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub(crate) help: Option<String>,
    #[prost(enumeration = "MetricType", optional, tag = "3")]
    pub(crate) r#type: Option<i32>,
    #[prost(message, repeated, tag = "4")]
    pub(crate) metric: Vec<Metric>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct Metric {
    #[prost(message, repeated, tag = "1")]
    pub(crate) label: Vec<LabelPair>,
    #[prost(message, optional, tag = "2")]
    pub(crate) gauge: Option<Gauge>,
    #[prost(message, optional, tag = "3")]
    pub(crate) counter: Option<Counter>,
    #[prost(message, optional, tag = "7")]
    pub(crate) histogram: Option<Histogram>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct LabelPair {
    #[prost(string, optional, tag = "1")]
    pub(crate) name: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub(crate) value: Option<String>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct Counter {
    #[prost(double, optional, tag = "1")]
    pub(crate) value: Option<f64>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct Gauge {
    #[prost(double, optional, tag = "1")]
    pub(crate) value: Option<f64>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct Histogram {
    #[prost(uint64, optional, tag = "1")]
    pub(crate) sample_count: Option<u64>,
    #[prost(double, optional, tag = "2")]
    pub(crate) sample_sum: Option<f64>,
    #[prost(message, repeated, tag = "3")]
    pub(crate) bucket: Vec<Bucket>,
    #[prost(sint32, optional, tag = "5")]
    pub(crate) schema: Option<i32>,
    #[prost(double, optional, tag = "6")]
    pub(crate) zero_threshold: Option<f64>,
    #[prost(uint64, optional, tag = "7")]
    pub(crate) zero_count: Option<u64>,
    #[prost(message, repeated, tag = "12")]
    pub(crate) positive_span: Vec<BucketSpan>,
    #[prost(sint64, repeated, packed = "false", tag = "13")]
    pub(crate) positive_delta: Vec<i64>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct Bucket {
    #[prost(uint64, optional, tag = "1")]
    pub(crate) cumulative_count: Option<u64>,
    #[prost(double, optional, tag = "2")]
    pub(crate) upper_bound: Option<f64>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct BucketSpan {
    #[prost(sint32, optional, tag = "1")]
    pub(crate) offset: Option<i32>,
    #[prost(uint32, optional, tag = "2")]
    pub(crate) length: Option<u32>,
}

pub(crate) fn label(name: &str, value: &str) -> LabelPair {
    LabelPair {
        name: Some(name.to_string()),
        value: Some(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use crate::prometheus_server::protobuf::{label, Gauge, Metric, MetricFamily, MetricType};

    // Golden bytes hand-derived from the protobuf wire format:
    // field 1 (name, len-delimited): 0x0a len "up"
    // field 3 (type, varint):        0x18 0x01 (GAUGE)
    // field 4 (metric, len-delim):   0x22 len <Metric>
    //   Metric field 1 (label):      0x0a len <LabelPair name="a" value="b">
    //   Metric field 2 (gauge):      0x12 0x09 <Gauge field 1 fixed64 1.0>
    #[test]
    fn golden_gauge_family() {
        let family = MetricFamily {
            name: Some("up".into()),
            help: None,
            r#type: Some(MetricType::Gauge as i32),
            metric: vec![Metric {
                label: vec![label("a", "b")],
                gauge: Some(Gauge { value: Some(1.0) }),
                counter: None,
                histogram: None,
            }],
        };
        let bytes = family.encode_to_vec();
        let expected: &[u8] = &[
            0x0a, 0x02, b'u', b'p', // name = "up"
            0x18, 0x01, // type = GAUGE
            0x22, 0x13, // metric, 19 bytes
            0x0a, 0x06, // label, 6 bytes
            0x0a, 0x01, b'a', // label.name = "a"
            0x12, 0x01, b'b', // label.value = "b"
            0x12, 0x09, // gauge, 9 bytes
            0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f, // value = 1.0
        ];
        assert_eq!(bytes, expected);
    }
}
