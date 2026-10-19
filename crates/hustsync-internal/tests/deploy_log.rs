//! The `[log]` sinks shipped in `deploy/` build as serlog slots, so a mistyped key fails here, not in production.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

#[test]
fn shipped_log_tables_build() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for file in ["deploy/compose/manager.toml", "deploy/compose/worker.toml"] {
        let text = std::fs::read_to_string(repo.join(file)).unwrap();
        let table: toml::Table = text.parse().unwrap();
        let sinks = table["log"]["sinks"].as_array().expect(file);
        assert!(!sinks.is_empty(), "{file}");
        for entry in sinks {
            let tree = serde_json::to_value(entry).unwrap();
            if let Err(e) = serlog::config::slot(tree) {
                panic!("{file}: {e}");
            }
        }
    }
}
