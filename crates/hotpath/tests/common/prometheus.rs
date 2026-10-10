use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;

/// GETs `path` from the Prometheus server on `port`; error statuses are
/// returned, not raised.
pub(crate) fn get(port: &str, path: &str) -> Result<(u16, String), ureq::Error> {
    let url = format!("http://localhost:{port}{path}");
    let mut response = ureq::get(&url)
        .config()
        .http_status_as_error(false)
        .build()
        .call()?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string()?;
    Ok((status, body))
}

/// Scrapes `/metrics` every 750ms until `ready` accepts a 200 body.
#[track_caller]
pub(crate) fn scrape_until(port: &str, attempts: u32, ready: impl Fn(&str) -> bool) -> String {
    for _attempt in 0..attempts {
        sleep(Duration::from_millis(750));
        if let Ok((200, body)) = get(port, "/metrics") {
            if ready(&body) {
                return body;
            }
        }
    }
    panic!("Prometheus server did not serve the expected metrics on port {port}");
}

/// Splits a sample line into `(name, labels, value)`, tolerating `}`
/// inside label values (route templates like `GET /users/{id}`).
pub(crate) fn parse_line(line: &str) -> Option<(&str, &str, &str)> {
    let (series, value) = line.rsplit_once(' ')?;
    match series.split_once('{') {
        Some((name, rest)) => Some((name, rest.strip_suffix('}')?, value)),
        None => Some((series, "", value)),
    }
}

pub(crate) fn label_value(labels: &str, name: &str) -> String {
    let start = labels.find(&format!("{}=\"", name)).unwrap() + name.len() + 2;
    labels[start..].split('"').next().unwrap().to_string()
}

/// Value of the first `family` series whose labels contain every needle.
#[track_caller]
pub(crate) fn series_value(body: &str, family: &str, needles: &[&str]) -> f64 {
    body.lines()
        .find_map(|l| {
            parse_line(l).filter(|(name, labels, _)| {
                *name == family && needles.iter().all(|n| labels.contains(n))
            })
        })
        .map(|(_, _, value)| value.parse().unwrap())
        .unwrap_or_else(|| panic!("{family} series matching {needles:?} missing"))
}

/// Every non-comment line is `name{labels} value` or `name value`.
#[track_caller]
pub(crate) fn assert_sample_lines_parse(body: &str) {
    for line in body
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let (_, _, value) = parse_line(line).expect("line has no value");
        assert!(
            value.parse::<f64>().is_ok(),
            "unparsable value in line: {line}"
        );
    }
}

/// For one histogram family: `le` strictly increasing with `+Inf` last,
/// bucket counts non-decreasing, `+Inf` bucket == `_count`, per series.
#[track_caller]
pub(crate) fn assert_histogram_family(body: &str, family: &str) {
    let bucket_name = format!("{family}_bucket");
    let count_name = format!("{family}_count");
    // labels-without-le -> (le, cumulative count) pairs in exposition order
    let mut buckets: HashMap<String, Vec<(f64, u64)>> = HashMap::new();
    let mut counts: HashMap<String, u64> = HashMap::new();

    for line in body.lines().filter(|l| !l.starts_with('#')) {
        let Some((name, labels, value)) = parse_line(line) else {
            continue;
        };
        if name == bucket_name {
            let le = label_value(labels, "le");
            let le = if le == "+Inf" {
                f64::INFINITY
            } else {
                le.parse().unwrap()
            };
            let key = labels[..labels.rfind(",le=\"").expect("le must be last")].to_string();
            buckets
                .entry(key)
                .or_default()
                .push((le, value.parse().unwrap()));
        } else if name == count_name {
            counts.insert(labels.to_string(), value.parse().unwrap());
        }
    }

    assert!(!buckets.is_empty(), "{family}: no bucket series found");
    for (series, pairs) in &buckets {
        for pair in pairs.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "{family}{{{series}}}: le not strictly increasing: {:?}",
                pair
            );
            assert!(
                pair[0].1 <= pair[1].1,
                "{family}{{{series}}}: bucket counts decreasing: {:?}",
                pair
            );
        }
        let (last_le, last_count) = *pairs.last().unwrap();
        assert!(
            last_le.is_infinite(),
            "{family}{{{series}}}: last bucket must be +Inf"
        );
        assert_eq!(
            last_count, counts[series],
            "{family}{{{series}}}: +Inf bucket != _count"
        );
    }
}
