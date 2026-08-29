//! hdrhistogram -> Prometheus native-histogram conversion for the
//! `hotpath-prometheus` exporter. Native bucket `i` covers
//! `(2^((i-1)/2^schema), 2^(i/2^schema)]`, values in seconds.

/// Smallest native-histogram bucket index `i` such that `2^(i / 2^schema) >= v`,
/// for `v` in seconds. Uses a frexp-style decomposition so exact powers of two
/// index exactly (fraction 0.5 gives `log2 == -1.0` with no rounding); the
/// other boundaries are irrational, so a value can never sit exactly on one.
fn native_bucket_index(v_seconds: f64, schema: i32) -> i32 {
    let bits = v_seconds.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32 - 1022;
    let frac = f64::from_bits((bits & 0x800f_ffff_ffff_ffff) | (1022u64 << 52));
    exp * (1 << schema) + (frac.log2() * (1i64 << schema) as f64).ceil() as i32
}

/// One ascending pass over the recorded hdr bins -> sparse native-histogram
/// buckets `(index, count)` at `schema`, indices sorted. hdr resolves 0.1%,
/// finer than schema 3's 9% buckets, so adjacent bins usually collapse into
/// one native bucket. Bins are indexed by their upper edge, matching the `le`
/// convention.
pub(crate) fn native_bucket_counts(
    hist: &hdrhistogram::Histogram<u64>,
    schema: i32,
) -> Vec<(i32, u64)> {
    let mut out: Vec<(i32, u64)> = Vec::new();
    for v in hist.iter_recorded() {
        let idx = native_bucket_index(v.value_iterated_to() as f64 / 1e9, schema);
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

/// Coarse classic buckets for the fallback path: each native bucket's count
/// goes to the ladder bucket containing the native bucket's upper bound
/// (first boundary `>= 2^(idx/2^schema)` seconds), then counts are cumulated.
/// A native bucket above the top boundary lands only in the implicit `+Inf`.
pub(crate) fn classic_from_native(
    sparse: &[(i32, u64)],
    schema: i32,
    ladder_ns: &[u64],
) -> Vec<u64> {
    let mut counts = vec![0u64; ladder_ns.len()];
    for &(idx, count) in sparse {
        let upper_ns = 2f64.powf(idx as f64 / (1i64 << schema) as f64) * 1e9;
        if let Some(pos) = ladder_ns.iter().position(|&b| b as f64 >= upper_ns) {
            counts[pos] += count;
        }
    }
    for i in 1..counts.len() {
        counts[i] += counts[i - 1];
    }
    counts
}

#[cfg(test)]
mod tests {
    use hdrhistogram::Histogram;

    use crate::lib_on::native_histograms::{
        classic_from_native, native_bucket_counts, native_bucket_index, to_spans,
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
        let sparse = native_bucket_counts(&hist, 3);
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
    fn classic_from_native_is_cumulative_and_conserving() {
        let ladder: &[u64] = &[1_000, 1_000_000, 1_000_000_000];
        let sparse = vec![
            (native_bucket_index(500e-9, 3), 10u64), // ~500ns -> 1us bucket
            (native_bucket_index(0.5e-3, 3), 20),    // ~0.5ms -> 1ms bucket
            (native_bucket_index(0.9, 3), 30),       // ~0.9s -> 1s bucket
            (native_bucket_index(5.0, 3), 40),       // 5s -> +Inf only
        ];
        let counts = classic_from_native(&sparse, 3, ladder);
        assert_eq!(counts, vec![10, 30, 60]);
        assert!(counts.windows(2).all(|w| w[0] <= w[1]));
        assert!(counts.last().unwrap() <= &100);
    }
}
