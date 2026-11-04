//! `[metrics]`: per-group cache times and the targets metrics are pushed to.

use std::collections::BTreeMap;

use serde::Deserialize;

/// `[metrics]` of the manager or the worker.
#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    /// Seconds a group stays cached after it is gathered; groups not listed are gathered per request.
    #[serde(default)]
    pub cache: BTreeMap<String, u64>,
    /// `[[metrics.push]]`, one entry per target.
    #[serde(default)]
    pub push: Vec<PushConfig>,
}

/// One `[[metrics.push]]` target.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PushConfig {
    pub url: String,
    /// Seconds between pushes.
    #[serde(default = "PushConfig::interval")]
    pub interval: u64,
    /// Groups to push; all when unset.
    pub groups: Option<Vec<String>>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

impl PushConfig {
    const fn interval() -> u64 {
        30
    }
}
