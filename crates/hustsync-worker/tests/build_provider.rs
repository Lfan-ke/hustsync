//! Defaults `build_provider` fills in from tunasync.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use hustsync_config_parser::{MirrorConfig, WorkerConfig};
use hustsync_worker::provider::build_provider;

#[test]
fn timeout_defaults_to_none() {
    let m = MirrorConfig {
        name: Some("m".into()),
        provider: Some("command".into()),
        command: Some("true".into()),
        ..MirrorConfig::default()
    };
    let p = build_provider("m", &m, &WorkerConfig::default()).unwrap();
    assert_eq!(p.timeout(), Duration::ZERO);
}
