/// HdrHistogram V2 deflate payload, base64-encoded. Decodable by any HdrHistogram
/// implementation (`hdrhistogram::serialization::Deserializer`, hdr-histogram-js).
#[cfg(feature = "hotpath-cloud")]
pub(crate) fn histogram_base64(hist: &hdrhistogram::Histogram<u64>) -> Option<String> {
    use base64::Engine;
    use hdrhistogram::serialization::{Serializer, V2DeflateSerializer};

    let mut buf = Vec::new();
    V2DeflateSerializer::new().serialize(hist, &mut buf).ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(&buf))
}

#[cfg(not(feature = "hotpath-cloud"))]
pub(crate) fn histogram_base64(_hist: &hdrhistogram::Histogram<u64>) -> Option<String> {
    None
}

/// Cumulative counts of recorded values at or below each of the ascending
/// `boundaries`, in one ordered traversal of the non-empty bins. Matches
/// `count_between(0, b)` per boundary: a bin straddling a boundary counts
/// toward it in full (the histogram's 0.1% resolution).
pub(crate) fn cumulative_bucket_counts(
    hist: &hdrhistogram::Histogram<u64>,
    boundaries: &[u64],
) -> Vec<u64> {
    let mut counts = Vec::with_capacity(boundaries.len());
    let mut cumulative: u64 = 0;
    for v in hist.iter_recorded() {
        let bin_low = hist.lowest_equivalent(v.value_iterated_to());
        while counts.len() < boundaries.len() && bin_low > boundaries[counts.len()] {
            counts.push(cumulative);
        }
        if counts.len() == boundaries.len() {
            break;
        }
        cumulative += v.count_since_last_iteration();
    }
    while counts.len() < boundaries.len() {
        counts.push(cumulative);
    }
    counts
}

#[cfg(all(test, feature = "hotpath-cloud"))]
pub(crate) fn decode_histogram(b64: &str) -> hdrhistogram::Histogram<u64> {
    use base64::Engine;
    use hdrhistogram::serialization::Deserializer;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    Deserializer::new().deserialize(&mut &bytes[..]).unwrap()
}

#[cfg(test)]
mod bucket_counts_tests {
    use hdrhistogram::Histogram;

    use crate::lib_on::histograms::cumulative_bucket_counts;

    #[test]
    fn matches_count_between_per_boundary() {
        let boundaries = [
            250u64,
            1_000,
            25_000,
            1_000_000,
            500_000_000,
            10_000_000_000,
        ];
        let mut hist = Histogram::<u64>::new_with_bounds(1, 1_000_000_000_000, 3).unwrap();
        let mut x: u64 = 42;
        for _ in 0..10_000 {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            hist.record(200 + x % 2_000_000_000).unwrap();
        }
        // exercise the straddling-bin edge: values equivalent to a boundary
        hist.record(1_000).unwrap();
        hist.record(25_010).unwrap();

        let expected: Vec<u64> = boundaries
            .iter()
            .map(|&b| hist.count_between(0, b))
            .collect();
        assert_eq!(cumulative_bucket_counts(&hist, &boundaries), expected);
    }

    #[test]
    fn empty_histogram_yields_zeroes() {
        let hist = Histogram::<u64>::new_with_bounds(1, 1_000_000_000_000, 3).unwrap();
        assert_eq!(cumulative_bucket_counts(&hist, &[100, 1_000]), vec![0, 0]);
    }
}

#[cfg(all(test, feature = "hotpath-cloud"))]
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
