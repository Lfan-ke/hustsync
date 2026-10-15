//! Status messages a job reports while syncing.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{collections::HashMap, sync::Arc, time::Duration};

use hustsync_internal::status::SyncStatus;
use hustsync_worker::{
    JobMessage,
    job::{CtrlAction, JobActor},
    provider::{
        CommonProviderConfig,
        cmd_provider::{CmdProvider, CmdProviderConfig},
    },
};
use tempfile::TempDir;
use tokio::sync::{Semaphore, mpsc};

/// Runs `command` once through a job actor and collects reports until the last retry.
async fn reports(command: &str, retry: u32) -> Vec<(SyncStatus, bool)> {
    let dir = TempDir::new().unwrap();
    let path = |p: &str| dir.path().join(p).to_string_lossy().into_owned();
    let provider = CmdProvider::new(CmdProviderConfig {
        common: CommonProviderConfig {
            name: "m".into(),
            upstream_url: String::new(),
            working_dir: path("data"),
            log_dir: path("log"),
            log_file: path("log/latest.log"),
            interval: Duration::from_secs(3600),
            retry,
            timeout: Duration::from_secs(30),
            env: HashMap::new(),
            is_master: true,
            success_exit_codes: vec![],
        },
        command: command.into(),
        fail_on_match: None,
        size_pattern: None,
    })
    .unwrap();
    let (tx, mut rx) = mpsc::channel::<JobMessage>(32);
    let (job, actor) = JobActor::new(
        "m".into(),
        tx,
        Arc::new(Semaphore::new(1)),
        Box::new(provider),
        vec![],
    );
    tokio::spawn(actor.run());
    job.send_ctrl(CtrlAction::Start).await.unwrap();

    let mut seen = vec![];
    while let Ok(Some(m)) = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
        seen.push((m.status, m.schedule));
        if m.schedule {
            break;
        }
    }
    seen
}

#[tokio::test]
async fn retries_report_pre_syncing_once() {
    use SyncStatus::*;
    assert_eq!(
        reports("false", 2).await,
        [
            (PreSyncing, false),
            (Syncing, false),
            (Failed, false),
            (Syncing, false),
            (Failed, true),
        ]
    );
}
