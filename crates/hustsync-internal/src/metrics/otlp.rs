//! OTLP/HTTP JSON bodies for `/v1/metrics`.

use std::time::{SystemTime, UNIX_EPOCH};

use prometheus::proto::{LabelPair, Metric, MetricFamily, MetricType};
use serde_json::{Value, json};

use super::{Families, push::Origin};

/// An `ExportMetricsServiceRequest` holding every metric of the groups.
pub fn encode(groups: &[Families], origin: &Origin, at: SystemTime) -> Value {
    let (now, start) = (nanos(at), nanos(origin.start));
    let metrics: Vec<_> = groups
        .iter()
        .flat_map(|g| g.iter())
        .filter_map(|f| family(f, &now, &start))
        .collect();
    json!({
        "resourceMetrics": [{
            "resource": { "attributes": attrs([
                ("service.name", origin.service),
                ("service.instance.id", &origin.instance),
                ("service.version", env!("CARGO_PKG_VERSION")),
            ]) },
            "scopeMetrics": [{
                "scope": { "name": "hustsync", "version": env!("CARGO_PKG_VERSION") },
                "metrics": metrics,
            }],
        }],
    })
}

fn family(f: &MetricFamily, now: &str, start: &str) -> Option<Value> {
    let ms = f.get_metric();
    let points = |p: fn(&Metric) -> Option<Value>| -> Vec<Value> {
        ms.iter()
            .filter_map(|m| {
                let mut v = p(m)?;
                v["attributes"] = labels(m.get_label());
                v["timeUnixNano"] = when(m, now).into();
                v["startTimeUnixNano"] = start.into();
                Some(v)
            })
            .collect()
    };
    let (name, kind, data) = match f.get_field_type() {
        MetricType::UNTYPED => return None,
        MetricType::GAUGE => (
            f.name(),
            "gauge",
            json!({ "dataPoints": points(|m| double(m.get_gauge().get_value())) }),
        ),
        MetricType::COUNTER => (
            f.name().trim_end_matches("_total"),
            "sum",
            json!({
                "aggregationTemporality": 2,
                "isMonotonic": true,
                "dataPoints": points(|m| double(m.get_counter().get_value())),
            }),
        ),
        MetricType::HISTOGRAM => (
            f.name(),
            "histogram",
            json!({ "aggregationTemporality": 2, "dataPoints": points(histogram) }),
        ),
        MetricType::SUMMARY => (
            f.name(),
            "summary",
            json!({ "dataPoints": points(summary) }),
        ),
    };
    let mut out = json!({ "name": name, "description": f.help(), "unit": "" });
    out[kind] = data;
    Some(out)
}

fn double(v: f64) -> Option<Value> {
    v.is_finite().then(|| json!({ "asDouble": v }))
}

fn histogram(m: &Metric) -> Option<Value> {
    let h = m.get_histogram();
    let mut bounds = Vec::new();
    let mut counts = Vec::new();
    let mut seen = 0;
    for b in h.get_bucket() {
        if b.upper_bound().is_finite() {
            bounds.push(b.upper_bound());
            counts.push((b.cumulative_count() - seen).to_string());
            seen = b.cumulative_count();
        }
    }
    counts.push((h.get_sample_count() - seen).to_string());
    Some(json!({
        "count": h.get_sample_count().to_string(),
        "sum": h.get_sample_sum(),
        "bucketCounts": counts,
        "explicitBounds": bounds,
    }))
}

fn summary(m: &Metric) -> Option<Value> {
    let s = m.get_summary();
    let qs: Vec<_> = s
        .get_quantile()
        .iter()
        .map(|q| json!({ "quantile": q.quantile(), "value": q.value() }))
        .collect();
    Some(json!({
        "count": s.sample_count().to_string(),
        "sum": s.sample_sum(),
        "quantileValues": qs,
    }))
}

fn labels(ls: &[LabelPair]) -> Value {
    attrs(ls.iter().map(|l| (l.name(), l.value())))
}

fn attrs<'a>(kv: impl IntoIterator<Item = (&'a str, &'a str)>) -> Value {
    kv.into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| json!({ "key": k, "value": { "stringValue": v } }))
        .collect()
}

fn when(m: &Metric, now: &str) -> String {
    match m.timestamp_ms() {
        0 => now.to_owned(),
        ms => (i128::from(ms) * 1_000_000).to_string(),
    }
}

fn nanos(t: SystemTime) -> String {
    t.duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
        .to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use prometheus::{HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, core::Collector};

    use super::*;

    #[test]
    fn families_map_to_otlp_kinds() {
        let c =
            IntCounterVec::new(Opts::new("hustsync_syncs_total", "Syncs."), &["mirror"]).unwrap();
        c.with_label_values(&["debian"]).inc_by(3);
        let g = IntGauge::new("hustsync_workers", "Workers.").unwrap();
        g.set(2);
        let h = HistogramVec::new(
            HistogramOpts::new("d", "D.").buckets(vec![1.0, 60.0]),
            &["w"],
        )
        .unwrap();
        for v in [0.5, 30.0, 30.0, 90.0] {
            h.with_label_values(&["a"]).observe(v);
        }
        let fams: Arc<[_]> = [c.collect(), g.collect(), h.collect()].concat().into();
        let origin = Origin {
            service: "hustsync-manager",
            instance: "m1".into(),
            start: UNIX_EPOCH + Duration::from_secs(1),
        };
        let body = encode(&[fams], &origin, UNIX_EPOCH + Duration::from_secs(2));
        let rm = &body["resourceMetrics"][0];
        assert_eq!(
            rm["resource"]["attributes"][0]["value"]["stringValue"],
            "hustsync-manager"
        );
        let ms = &rm["scopeMetrics"][0]["metrics"];
        assert_eq!(ms[0]["name"], "hustsync_syncs");
        assert_eq!(ms[0]["sum"]["isMonotonic"], true);
        let p = &ms[0]["sum"]["dataPoints"][0];
        assert_eq!(p["asDouble"], 3.0);
        assert_eq!(p["attributes"][0]["key"], "mirror");
        assert_eq!(p["timeUnixNano"], "2000000000");
        assert_eq!(p["startTimeUnixNano"], "1000000000");
        assert_eq!(ms[1]["gauge"]["dataPoints"][0]["asDouble"], 2.0);
        let hp = &ms[2]["histogram"]["dataPoints"][0];
        assert_eq!(hp["count"], "4");
        assert_eq!(hp["explicitBounds"], json!([1.0, 60.0]));
        assert_eq!(hp["bucketCounts"], json!(["1", "2", "1"]));
    }
}
