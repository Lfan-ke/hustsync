//! Every worker example under `docs/example/*/` and `deploy/` loads and validates; top-level files there are parser fixtures.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

use hustsync_config_parser::{ConfigLoadOptions, load_worker_config, validate_worker_config};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            collect(&p, out);
        } else if p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains("worker"))
        {
            out.push(p);
        }
    }
}

#[test]
fn worker_examples_load_and_validate() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![repo.join("deploy/compose/worker.toml")];
    for entry in std::fs::read_dir(repo.join("docs/example")).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            collect(&p, &mut files);
        }
    }
    assert!(files.len() >= 5, "{files:?}");
    for f in &files {
        let cfg = load_worker_config(f, &ConfigLoadOptions::default())
            .unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        validate_worker_config(&cfg).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
    }
}
