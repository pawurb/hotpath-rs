//! hdrhistogram -> Prometheus native-histogram conversion for the
//! `hotpath-prometheus` exporter. Native bucket `i` covers
//! `(2^((i-1)/2^schema), 2^(i/2^schema)]`, values in the metric's base unit.

/// Divisor turning stored nanosecond values into the exported base unit
/// (seconds). Byte- and count-valued histograms are already in base units and
/// use [`UNIT_SCALE`].
pub(crate) const NANOS_SCALE: f64 = 1e9;
/// Identity scale for histograms whose stored values are already base units.
/// Only the alloc snapshot uses it today.
#[cfg_attr(not(feature = "hotpath-alloc"), allow(dead_code))]
pub(crate) const UNIT_SCALE: f64 = 1.0;

/// Smallest native-histogram bucket index `i` such that `2^(i / 2^schema) >= v`,
/// for `v` in base units. Uses a frexp-style decomposition so exact powers of
/// two index exactly (fraction 0.5 gives `log2 == -1.0` with no rounding); the
/// other boundaries are irrational, so a value can never sit exactly on one.
fn native_bucket_index(v: f64, schema: i32) -> i32 {
    let bits = v.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32 - 1022;
    let frac = f64::from_bits((bits & 0x800f_ffff_ffff_ffff) | (1022u64 << 52));
    exp * (1 << schema) + (frac.log2() * (1i64 << schema) as f64).ceil() as i32
}

/// One ascending pass over the recorded hdr bins -> sparse native-histogram
/// buckets `(index, count)` at `schema`, indices sorted. Stored values are
/// divided by `scale` to reach the exported base unit, so the buckets line up
/// with the family's `sample_sum` and classic bounds. hdr resolves 0.1%,
/// finer than schema 3's 9% buckets, so adjacent bins usually collapse into
/// one native bucket. Bins are indexed by their upper edge, matching the `le`
/// convention.
pub(crate) fn native_bucket_counts(
    hist: &hdrhistogram::Histogram<u64>,
    schema: i32,
    scale: f64,
) -> Vec<(i32, u64)> {
    let mut out: Vec<(i32, u64)> = Vec::new();
    for v in hist.iter_recorded() {
        let idx = native_bucket_index(v.value_iterated_to() as f64 / scale, schema);
        match out.last_mut() {
            Some((last, count)) if *last == idx => *count += v.count_since_last_iteration(),
            _ => out.push((idx, v.count_since_last_iteration())),
        }
    }
    out
}

/// Sparse buckets -> the protobuf span/delta encoding: spans of consecutive
/// indices (first span's `offset` is absolute, later offsets are the gap from
/// the previous span's end), counts as diffs from the previous bucket.
pub(crate) fn to_spans(sparse: &[(i32, u64)]) -> (Vec<(i32, u32)>, Vec<i64>) {
    let mut spans: Vec<(i32, u32)> = Vec::new();
    let mut deltas = Vec::with_capacity(sparse.len());
    let mut prev_count = 0i64;
    let mut prev_idx = None;
    for &(idx, count) in sparse {
        match prev_idx {
            Some(p) if idx == p + 1 => spans.last_mut().unwrap().1 += 1,
            Some(p) => spans.push((idx - p - 1, 1)),
            None => spans.push((idx, 1)),
        }
        deltas.push(count as i64 - prev_count);
        prev_count = count as i64;
        prev_idx = Some(idx);
    }
    (spans, deltas)
}

/// [`native_bucket_counts`] over an optional histogram: `None` or
/// `populated == false` (the caller's "anything sampled?" check) yields no
/// buckets.
pub(crate) fn native_buckets_opt(
    hist: Option<&hdrhistogram::Histogram<u64>>,
    populated: bool,
    schema: i32,
    scale: f64,
) -> Vec<(i32, u64)> {
    match hist.filter(|_| populated) {
        Some(hist) => native_bucket_counts(hist, schema, scale),
        None => Vec::new(),
    }
}

/// [`cumulative_bucket_counts`] over an optional histogram: `None` or
/// `populated == false` yields all-zero buckets.
pub(crate) fn classic_buckets_opt(
    hist: Option<&hdrhistogram::Histogram<u64>>,
    populated: bool,
    boundaries: &[u64],
) -> Vec<u64> {
    match hist.filter(|_| populated) {
        Some(hist) => cumulative_bucket_counts(hist, boundaries),
        None => vec![0; boundaries.len()],
    }
}

/// Cumulative counts of recorded values at or below each of the ascending
/// classic `boundaries` (ns), in one ordered traversal of the non-empty hdr
/// bins. Matches `count_between(0, b)` per boundary: a bin straddling a
/// boundary counts toward it in full (the histogram's 0.1% resolution).
/// Computed straight from the hdr histogram, never from the coarser native
/// buckets, so boundary-adjacent observations land in the correct bucket.
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

#[cfg(test)]
mod tests {
    use hdrhistogram::Histogram;

    use crate::lib_on::native_histograms::{
        cumulative_bucket_counts, native_bucket_counts, native_bucket_index, to_spans, NANOS_SCALE,
    };

    fn upper_bound(idx: i32, schema: i32) -> f64 {
        2f64.powf(idx as f64 / (1i64 << schema) as f64)
    }

    #[test]
    fn index_exact_at_powers_of_two() {
        assert_eq!(native_bucket_index(1.0, 3), 0);
        assert_eq!(native_bucket_index(2.0, 3), 8);
        assert_eq!(native_bucket_index(0.5, 3), -8);
        assert_eq!(native_bucket_index(4.0, 3), 16);
    }

    #[test]
    fn index_invariant_over_random_values() {
        let mut x: u64 = 42;
        for schema in [2, 3, 4] {
            for _ in 0..10_000 {
                x = x
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let v_ns = 1 + x % 1_000_000_000_000;
                let v = v_ns as f64 / 1e9;
                let idx = native_bucket_index(v, schema);
                let slop = 1.0 + 1e-12;
                assert!(
                    upper_bound(idx, schema) * slop >= v,
                    "ub({idx}) < {v} at schema {schema}"
                );
                assert!(
                    upper_bound(idx - 1, schema) < v * slop,
                    "ub({}) >= {v} at schema {schema}",
                    idx - 1
                );
            }
        }
    }

    #[test]
    fn sparse_counts_conserve_totals_and_sort() {
        let mut hist = Histogram::<u64>::new_with_bounds(1, 1_000_000_000_000, 3).unwrap();
        let mut x: u64 = 7;
        for _ in 0..50_000 {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            hist.record(250 + x % 2_000_000_000).unwrap();
        }
        let sparse = native_bucket_counts(&hist, 3, NANOS_SCALE);
        assert_eq!(sparse.iter().map(|&(_, c)| c).sum::<u64>(), hist.len());
        assert!(sparse.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn spans_round_trip() {
        let sparse = vec![(-3, 5u64), (-2, 7), (0, 1), (10, 2), (11, 4)];
        let (spans, deltas) = to_spans(&sparse);
        assert_eq!(spans, vec![(-3, 2), (1, 1), (9, 2)]);

        let mut rebuilt = Vec::new();
        let mut idx = 0i32;
        let mut count = 0i64;
        let mut deltas_iter = deltas.into_iter();
        for (i, &(offset, len)) in spans.iter().enumerate() {
            idx += offset + if i > 0 { 1 } else { 0 };
            for step in 0..len {
                count += deltas_iter.next().unwrap();
                rebuilt.push((idx + step as i32, count as u64));
            }
            idx += len as i32 - 1;
        }
        assert_eq!(rebuilt, sparse);
    }

    #[test]
    fn cumulative_counts_match_count_between_per_boundary() {
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
        // boundary-exact observations must count toward their own boundary
        hist.record(250).unwrap();
        hist.record(1_000).unwrap();

        let expected: Vec<u64> = boundaries
            .iter()
            .map(|&b| hist.count_between(0, b))
            .collect();
        assert_eq!(cumulative_bucket_counts(&hist, &boundaries), expected);
    }

    #[test]
    fn boundary_exact_value_lands_in_its_bucket() {
        // The regression from deriving classic counts out of native buckets: a
        // 250ns observation sits in the native bucket ending at ~260.7ns, so a
        // native-derived le="250ns" bucket reported 0. Direct hdr counting
        // must report 1.
        let mut hist = Histogram::<u64>::new_with_bounds(1, 1_000_000_000_000, 3).unwrap();
        hist.record(250).unwrap();
        assert_eq!(cumulative_bucket_counts(&hist, &[250, 1_000]), vec![1, 1]);
    }

    #[test]
    fn empty_histogram_yields_zeroes() {
        let hist = Histogram::<u64>::new_with_bounds(1, 1_000_000_000_000, 3).unwrap();
        assert_eq!(cumulative_bucket_counts(&hist, &[100, 1_000]), vec![0, 0]);
    }
}
