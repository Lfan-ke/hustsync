//! The optional `[log]` table reaches both configs as it is written, for serlog to read.

#![allow(clippy::unwrap_used)]

use std::io::Write;

use hustsync_config_parser::{ManagerConfig, WorkerConfig, parse_config};
use tempfile::NamedTempFile;

const LOG: &str = r#"
[log]
filter = "info"
sinks = [{ kind = "loki", url = "http://alloy:3100/loki/api/v1/push" }]
"#;

fn file(text: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(text.as_bytes()).unwrap();
    f
}

fn kind(log: Option<&toml::Table>) -> &str {
    log.unwrap()["sinks"].as_array().unwrap()[0]["kind"]
        .as_str()
        .unwrap()
}

#[test]
fn the_manager_keeps_its_log_table() {
    let base = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/example/manager.conf"
    ))
    .unwrap();
    let cfg: ManagerConfig = parse_config(file(&format!("{base}{LOG}")).path()).unwrap();
    assert_eq!(kind(cfg.log.as_ref()), "loki");
    assert_eq!(cfg.log.unwrap()["filter"].as_str(), Some("info"));
}

#[test]
fn the_worker_keeps_its_log_table() {
    let base = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/example/worker.conf"
    ))
    .unwrap();
    let cfg: WorkerConfig = parse_config(file(&format!("{LOG}{base}")).path()).unwrap();
    assert_eq!(kind(cfg.log.as_ref()), "loki");
}

#[test]
fn configs_without_one_have_none() {
    let cfg: ManagerConfig = parse_config(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/example/manager.conf"
    ))
    .unwrap();
    assert!(cfg.log.is_none());
}
