//! Status messages a job reports while syncing.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use hustsync_internal::status::SyncStatus;
use hustsync_worker::{
    JobMessage,
    hooks::{HookCtx, HookError, JobHook},
    job::{CtrlAction, JobActor},
    provider::{
        CommonProviderConfig,
        cmd_provider::{CmdProvider, CmdProviderConfig},
    },
};
use tempfile::TempDir;
use tokio::sync::{Semaphore, mpsc};

/// Runs `command` once through a job actor and collects reports up to the one that reschedules.
async fn reports(
    command: &str,
    retry: u32,
    hooks: Vec<Arc<dyn JobHook>>,
) -> Vec<(SyncStatus, bool, String)> {
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
        hooks,
    );
    tokio::spawn(actor.run());
    job.send_ctrl(CtrlAction::Start).await.unwrap();

    let mut seen = vec![];
    while let Ok(Some(m)) = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
        seen.push((m.status, m.schedule, m.msg));
        if m.schedule {
            break;
        }
    }
    seen
}

#[tokio::test]
async fn retries_report_pre_syncing_once() {
    use SyncStatus::*;
    let got: Vec<_> = reports("false", 2, vec![])
        .await
        .into_iter()
        .map(|(s, sched, _)| (s, sched))
        .collect();
    assert_eq!(
        got,
        [
            (PreSyncing, false),
            (Syncing, false),
            (Failed, false),
            (Syncing, false),
            (Failed, true),
        ]
    );
}

struct Broken;

#[async_trait]
impl JobHook for Broken {
    fn name(&self) -> &str {
        "broken"
    }

    async fn pre_job(&self, _ctx: &mut HookCtx) -> Result<(), HookError> {
        Err(HookError::config("broken", "no disk"))
    }
}

#[tokio::test]
async fn failed_pre_job_hook_reports_and_reschedules() {
    let got = reports("true", 2, vec![Arc::new(Broken)]).await;
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!((got[0].0, got[0].1), (SyncStatus::PreSyncing, false));
    assert_eq!((got[1].0, got[1].1), (SyncStatus::Failed, true));
    assert!(got[1].2.starts_with("error exec hook pre_job: "), "{got:?}");
}
