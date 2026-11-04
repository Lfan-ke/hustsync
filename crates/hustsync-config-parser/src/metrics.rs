//! `[metrics]`: per-group cache times.

use std::collections::BTreeMap;

use serde::Deserialize;

/// `[metrics]` of the manager or the worker.
#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    /// Seconds a group stays cached after it is gathered; groups not listed are gathered per request.
    #[serde(default)]
    pub cache: BTreeMap<String, u64>,
}
