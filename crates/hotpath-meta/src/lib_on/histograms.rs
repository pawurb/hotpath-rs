/// HdrHistogram V2 deflate payload, base64-encoded. Decodable by any HdrHistogram
/// implementation (`hdrhistogram::serialization::Deserializer`, hdr-histogram-js).
#[cfg(feature = "hotpath-cloud-meta")]
pub(crate) fn histogram_base64(hist: &hdrhistogram::Histogram<u64>) -> Option<String> {
    use base64::Engine;
    use hdrhistogram::serialization::{Serializer, V2DeflateSerializer};

    let mut buf = Vec::new();
    V2DeflateSerializer::new().serialize(hist, &mut buf).ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(&buf))
}

#[cfg(not(feature = "hotpath-cloud-meta"))]
pub(crate) fn histogram_base64(_hist: &hdrhistogram::Histogram<u64>) -> Option<String> {
    None
}

#[cfg(all(test, feature = "hotpath-cloud-meta"))]
pub(crate) fn decode_histogram(b64: &str) -> hdrhistogram::Histogram<u64> {
    use base64::Engine;
    use hdrhistogram::serialization::Deserializer;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    Deserializer::new().deserialize(&mut &bytes[..]).unwrap()
}

#[cfg(all(test, feature = "hotpath-cloud-meta"))]
mod tests {
    use base64::Engine;
    use hdrhistogram::serialization::Deserializer;
    use hdrhistogram::Histogram;

    use crate::lib_on::histograms::histogram_base64;

    #[test]
    fn histogram_base64_round_trips() {
        let mut hist = Histogram::<u64>::new_with_bounds(1, 1_000_000_000_000, 3).unwrap();
        for v in [180_000u64, 190_000, 1_400_000, 1_500_000, 9_000_000] {
            hist.record(v).unwrap();
        }

        let b64 = histogram_base64(&hist).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let decoded: Histogram<u64> = Deserializer::new().deserialize(&mut &bytes[..]).unwrap();

        assert_eq!(decoded.len(), hist.len());
        assert_eq!(decoded.max(), hist.max());
        assert_eq!(decoded.value_at_quantile(0.5), hist.value_at_quantile(0.5));
    }
}
