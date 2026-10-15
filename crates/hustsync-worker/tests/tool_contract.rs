//! fpsync and lsyncd mirrors against real tools; skipped when a tool is missing.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{path::Path, time::Duration};

use hustsync_config_parser::{MirrorConfig, WorkerConfig};
use hustsync_worker::provider::{MirrorProvider, ProviderError, RunContext, build_provider};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn has(bin: &str) -> bool {
    let ok = std::process::Command::new("sh")
        .args(["-c", &format!("command -v {bin}")])
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("skipped: {bin} not installed");
    }
    ok
}

fn mirror(tmp: &TempDir, provider: &str, opts: &[&str]) -> Box<dyn MirrorProvider> {
    let dir = |p: &str| tmp.path().join(p).to_string_lossy().into_owned();
    let opts = Some(opts.iter().map(|s| (*s).to_owned()).collect());
    let m = MirrorConfig {
        name: Some("m".into()),
        provider: Some(provider.into()),
        upstream: Some(dir("src")),
        mirror_dir: Some(dir("dst")),
        log_dir: Some(dir("log")),
        fpsync_options: opts.clone(),
        lsyncd_options: opts,
        ..MirrorConfig::default()
    };
    build_provider("m", &m, &WorkerConfig::default(), None).unwrap()
}

fn seed(src: &Path) {
    std::fs::create_dir_all(src.join("pool/main")).unwrap();
    for i in 0..20 {
        std::fs::write(src.join(format!("pool/main/f{i}")), format!("data {i}")).unwrap();
    }
}

async fn appears(file: &Path, secs: u64) -> bool {
    for _ in 0..secs * 10 {
        if file.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

#[tokio::test]
async fn fpsync_pulls_a_local_tree() {
    if !has("fpsync") {
        return;
    }
    let tmp = TempDir::new().unwrap();
    seed(&tmp.path().join("src"));
    let p = mirror(&tmp, "fpsync", &["-n", "2", "-f", "5"]);
    assert!(!p.daemon());
    p.run(RunContext::default()).await.unwrap();
    let got = std::fs::read_to_string(tmp.path().join("dst/pool/main/f7")).unwrap();
    assert_eq!(got, "data 7");
}

#[tokio::test]
async fn lsyncd_keeps_syncing_until_cancelled() {
    if !has("lsyncd") {
        return;
    }
    let tmp = TempDir::new().unwrap();
    seed(&tmp.path().join("src"));
    let p = mirror(&tmp, "lsyncd", &["-delay", "1"]);
    assert!(p.daemon());
    assert_eq!(p.timeout(), Duration::ZERO);

    let cancel = CancellationToken::new();
    let ctx = RunContext {
        cancel: cancel.clone(),
        ..RunContext::default()
    };
    let run = tokio::spawn(async move { p.run(ctx).await });

    assert!(
        appears(&tmp.path().join("dst/pool/main/f3"), 20).await,
        "initial sync"
    );
    std::fs::write(tmp.path().join("src/new"), "fresh").unwrap();
    assert!(appears(&tmp.path().join("dst/new"), 20).await, "live sync");

    cancel.cancel();
    let res = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(res, Err(ProviderError::Terminated)), "{res:?}");
}
