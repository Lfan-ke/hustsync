//! InfluxDB line protocol in the shape Telegraf's `inputs.prometheus` writes with `metric_version = 2`,
//! so pushed and scraped metrics come out of Telegraf alike.

use std::{
    fmt::Write,
    time::{SystemTime, UNIX_EPOCH},
};

use prometheus::proto::{MetricFamily, MetricType};

use super::Families;

/// Every metric of the groups, one line per sample, stamped `at` unless a metric carries its own time.
pub fn encode(groups: &[Families], at: SystemTime) -> String {
    let ns = at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let mut out = String::new();
    for f in groups.iter().flat_map(|g| g.iter()) {
        family(&mut out, f, ns);
    }
    out
}

fn family(out: &mut String, f: &MetricFamily, ns: u128) {
    let name = f.name();
    for m in f.get_metric() {
        let ns = match m.timestamp_ms() {
            0 => ns,
            ms => u128::try_from(ms).unwrap_or_default() * 1_000_000,
        };
        let tags: Vec<_> = m
            .get_label()
            .iter()
            .filter(|l| !l.value().is_empty())
            .map(|l| (l.name(), l.value().to_owned()))
            .collect();
        let line = |out: &mut String, extra: Option<(&str, String)>, fields: &[(String, f64)]| {
            let mut tags = tags.clone();
            tags.extend(extra);
            write_line(out, tags, fields, ns);
        };
        match f.get_field_type() {
            MetricType::COUNTER => line(out, None, &[(name.into(), m.get_counter().get_value())]),
            MetricType::GAUGE => line(out, None, &[(name.into(), m.get_gauge().get_value())]),
            MetricType::UNTYPED => {}
            MetricType::HISTOGRAM => {
                let h = m.get_histogram();
                let n = h.get_sample_count();
                line(
                    out,
                    None,
                    &[
                        (format!("{name}_count"), n as f64),
                        (format!("{name}_sum"), h.get_sample_sum()),
                    ],
                );
                let buckets = h.get_bucket().iter();
                let inf = h
                    .get_bucket()
                    .last()
                    .is_some_and(|b| b.upper_bound().is_infinite());
                let buckets = buckets
                    .map(|b| (b.upper_bound(), b.cumulative_count()))
                    .chain((!inf).then_some((f64::INFINITY, n)));
                for (le, c) in buckets {
                    line(
                        out,
                        Some(("le", go(le))),
                        &[(format!("{name}_bucket"), c as f64)],
                    );
                }
            }
            MetricType::SUMMARY => {
                let s = m.get_summary();
                line(
                    out,
                    None,
                    &[
                        (format!("{name}_sum"), s.sample_sum()),
                        (format!("{name}_count"), s.sample_count() as f64),
                    ],
                );
                for q in s.get_quantile() {
                    line(
                        out,
                        Some(("quantile", go(q.quantile()))),
                        &[(name.into(), q.value())],
                    );
                }
            }
        }
    }
}

fn write_line(out: &mut String, mut tags: Vec<(&str, String)>, fields: &[(String, f64)], ns: u128) {
    let fields: Vec<_> = fields.iter().filter(|(_, v)| v.is_finite()).collect();
    if fields.is_empty() {
        return;
    }
    tags.sort_unstable_by(|a, b| a.0.cmp(b.0));
    out.push_str("prometheus");
    for (k, v) in &tags {
        let _ = write!(out, ",{}={}", escape(k), escape(v));
    }
    for (i, (k, v)) in fields.iter().enumerate() {
        let _ = write!(out, "{}{}={v}", if i == 0 { ' ' } else { ',' }, escape(k));
    }
    let _ = writeln!(out, " {ns}");
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, ',' | '=' | ' ') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A float as Go's `fmt.Sprint` writes it, which is how Telegraf spells `le` and `quantile`.
fn go(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "+Inf" } else { "-Inf" }.into();
    }
    let e = format!("{v:e}");
    let Some((mant, exp)) = e.split_once('e') else {
        return e;
    };
    let exp: i32 = exp.parse().unwrap_or_default();
    if v == 0.0 || (-4..6).contains(&exp) {
        return format!("{v}");
    }
    let sign = if exp < 0 { '-' } else { '+' };
    format!("{mant}e{sign}{:02}", exp.abs())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use prometheus::{GaugeVec, HistogramOpts, HistogramVec, IntCounterVec, Opts, core::Collector};

    use super::*;

    const AT: u64 = 1_792_369_124;

    fn at() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(AT)
    }

    #[test]
    fn go_floats() {
        for (v, s) in [
            (1.0, "1"),
            (0.5, "0.5"),
            (60.0, "60"),
            (86_400.0, "86400"),
            (1e6, "1e+06"),
            (123_456_789.0, "1.23456789e+08"),
            (2.5e-5, "2.5e-05"),
            (0.0001, "0.0001"),
            (0.0, "0"),
            (f64::INFINITY, "+Inf"),
        ] {
            assert_eq!(go(v), s, "{v}");
        }
    }

    #[test]
    fn lines_match_telegraf() {
        let syncs = IntCounterVec::new(Opts::new("s_total", "s"), &["mirror", "worker"]).unwrap();
        syncs.with_label_values(&["debian", "w 1"]).inc_by(5);
        let g = GaugeVec::new(Opts::new("g", "g"), &["b", "a"]).unwrap();
        g.with_label_values(&["v", ""]).set(1e-7);
        g.with_label_values(&["x,y=z", "1"]).set(f64::NAN);
        let h = HistogramVec::new(HistogramOpts::new("h", "h").buckets(vec![0.5, 1e6]), &["w"])
            .unwrap();
        h.with_label_values(&["1"]).observe(0.1);
        h.with_label_values(&["1"]).observe(7.0);
        let fams: Arc<[_]> = [syncs.collect(), g.collect(), h.collect()].concat().into();
        let ns = AT * 1_000_000_000;
        assert_eq!(
            encode(&[fams], at()),
            format!(
                "prometheus,mirror=debian,worker=w\\ 1 s_total=5 {ns}\n\
                 prometheus,b=v g=0.0000001 {ns}\n\
                 prometheus,w=1 h_count=2,h_sum=7.1 {ns}\n\
                 prometheus,le=0.5,w=1 h_bucket=1 {ns}\n\
                 prometheus,le=1e+06,w=1 h_bucket=2 {ns}\n\
                 prometheus,le=+Inf,w=1 h_bucket=2 {ns}\n"
            )
        );
    }
}
