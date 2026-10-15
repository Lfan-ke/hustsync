//! Status messages a job reports while syncing.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use hustsync_internal::status::SyncStatus;
use hustsync_worker::{
    JobMessage, MirrorJob,
    hooks::{HookCtx, HookError, JobHook},
    job::{CtrlAction, JobActor, STATE_PAUSED},
    provider::{
        CommonProviderConfig, ProviderError, ProviderType,
        cmd_provider::{CmdProvider, CmdProviderConfig},
    },
};
use tempfile::TempDir;
use tokio::sync::{Semaphore, mpsc};

/// A job actor whose provider `make` builds, sharing a semaphore with `permits` slots.
fn spawn_with(
    dir: &TempDir,
    retry: u32,
    hooks: Vec<Arc<dyn JobHook>>,
    permits: usize,
    make: impl FnOnce(CmdProviderConfig) -> Result<CmdProvider, ProviderError>,
) -> (MirrorJob, mpsc::Receiver<JobMessage>, Arc<Semaphore>) {
    let path = |p: &str| dir.path().join(p).to_string_lossy().into_owned();
    let provider = make(CmdProviderConfig {
        common: CommonProviderConfig {
            name: "m".into(),
            upstream_url: String::new(),
            working_dir: path("data"),
            log_dir: path("log"),
            log_file: path("log/latest.log"),
            interval: Duration::from_secs(3600),
            retry,
            timeout: Duration::from_secs(60),
            env: HashMap::new(),
            is_master: true,
            success_exit_codes: vec![],
            runner: Default::default(),
        },
        command: String::new(),
        fail_on_match: None,
        size_pattern: None,
    })
    .unwrap();
    let (tx, rx) = mpsc::channel::<JobMessage>(32);
    let sem = Arc::new(Semaphore::new(permits));
    let (job, actor) = JobActor::new("m".into(), tx, Arc::clone(&sem), Box::new(provider), hooks);
    tokio::spawn(actor.run());
    (job, rx, sem)
}

/// A job actor running `command`.
fn spawn(
    dir: &TempDir,
    command: &str,
    retry: u32,
    hooks: Vec<Arc<dyn JobHook>>,
    permits: usize,
) -> (MirrorJob, mpsc::Receiver<JobMessage>, Arc<Semaphore>) {
    spawn_with(dir, retry, hooks, permits, |mut cfg| {
        cfg.command = command.into();
        CmdProvider::new(cfg)
    })
}

async fn next(rx: &mut mpsc::Receiver<JobMessage>, secs: u64) -> Option<JobMessage> {
    tokio::time::timeout(Duration::from_secs(secs), rx.recv())
        .await
        .ok()
        .flatten()
}

/// Runs `command` once and collects reports up to the one that reschedules.
async fn reports(
    command: &str,
    retry: u32,
    hooks: Vec<Arc<dyn JobHook>>,
) -> Vec<(SyncStatus, bool, String)> {
    let dir = TempDir::new().unwrap();
    let (job, mut rx, _sem) = spawn(&dir, command, retry, hooks, 1);
    job.send_ctrl(CtrlAction::Start).await.unwrap();
    let mut seen = vec![];
    while let Some(m) = next(&mut rx, 10).await {
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

#[tokio::test]
async fn stop_while_waiting_for_a_slot_skips_the_run() {
    let dir = TempDir::new().unwrap();
    let (job, mut rx, sem) = spawn(&dir, "true", 1, vec![], 1);
    let held = Arc::clone(&sem).acquire_owned().await.unwrap();
    job.send_ctrl(CtrlAction::Start).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    job.send_ctrl(CtrlAction::Stop).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(held);
    assert!(next(&mut rx, 2).await.is_none());
    assert_eq!(job.state(), STATE_PAUSED);
}

#[tokio::test]
async fn restart_cancels_the_running_sync() {
    let dir = TempDir::new().unwrap();
    let (job, mut rx, _sem) = spawn(&dir, "sleep 30", 1, vec![], 1);
    job.send_ctrl(CtrlAction::Start).await.unwrap();
    assert_eq!(
        next(&mut rx, 5).await.unwrap().status,
        SyncStatus::PreSyncing
    );
    assert_eq!(next(&mut rx, 5).await.unwrap().status, SyncStatus::Syncing);
    job.send_ctrl(CtrlAction::Restart).await.unwrap();
    assert_eq!(
        next(&mut rx, 10).await.unwrap().status,
        SyncStatus::PreSyncing
    );
}

#[tokio::test]
async fn stop_during_sync_reports_nothing() {
    let dir = TempDir::new().unwrap();
    let (job, mut rx, _sem) = spawn(&dir, "sleep 30", 1, vec![], 1);
    job.send_ctrl(CtrlAction::Start).await.unwrap();
    assert_eq!(
        next(&mut rx, 5).await.unwrap().status,
        SyncStatus::PreSyncing
    );
    assert_eq!(next(&mut rx, 5).await.unwrap().status, SyncStatus::Syncing);
    job.send_ctrl(CtrlAction::Stop).await.unwrap();
    assert!(next(&mut rx, 5).await.is_none());
    assert_eq!(job.state(), STATE_PAUSED);
}

#[tokio::test]
async fn daemons_run_without_a_slot() {
    let dir = TempDir::new().unwrap();
    let (job, mut rx, sem) = spawn_with(&dir, 1, vec![], 1, |cfg| {
        CmdProvider::with_argv(cfg, ProviderType::Lsyncd, vec!["sleep".into(), "30".into()])
    });
    let _held = Arc::clone(&sem).acquire_owned().await.unwrap();
    job.send_ctrl(CtrlAction::Start).await.unwrap();
    assert_eq!(
        next(&mut rx, 5).await.unwrap().status,
        SyncStatus::PreSyncing
    );
    assert_eq!(next(&mut rx, 5).await.unwrap().status, SyncStatus::Syncing);
    job.send_ctrl(CtrlAction::Stop).await.unwrap();
}
