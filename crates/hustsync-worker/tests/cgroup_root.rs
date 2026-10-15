//! Jobs inside real cgroups; needs root: `sudo <test binary> --ignored --test-threads=1`.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use hustsync_config_parser::{MemBytes, WorkerCgroupConfig};
use hustsync_worker::{
    cgroup::{Cgroups, Version},
    provider::{
        CommonProviderConfig, MirrorProvider, ProviderError, RunContext, Runner,
        cmd_provider::{CmdProvider, CmdProviderConfig},
    },
};
use tempfile::TempDir;

const BASE: &str = "/sys/fs/cgroup";

/// A parent group of its own for one test, removed on drop.
struct Parent(PathBuf);

impl Drop for Parent {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn setup(group: &str) -> Option<(Arc<Cgroups>, Parent)> {
    if !nix::unistd::geteuid().is_root() {
        eprintln!("skipped: needs root");
        return None;
    }
    let v2 = Path::new(BASE).join("cgroup.controllers").exists();
    let dir = if v2 {
        let _ = std::fs::write(Path::new(BASE).join("cgroup.subtree_control"), "+memory");
        Path::new(BASE).join(group)
    } else {
        Path::new(BASE).join("memory").join(group)
    };
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = WorkerCgroupConfig {
        enable: Some(true),
        base_path: Some(BASE.into()),
        group: Some(group.into()),
    };
    let cg = Cgroups::init(&cfg).unwrap();
    assert_eq!(cg.version() == Version::V2, v2);
    Some((Arc::new(cg), Parent(dir)))
}

fn provider(
    dir: &TempDir,
    name: &str,
    cmd: &str,
    cg: &Arc<Cgroups>,
    mem: Option<MemBytes>,
) -> CmdProvider {
    let path = |p: &str| dir.path().join(p).to_string_lossy().into_owned();
    CmdProvider::new(CmdProviderConfig {
        common: CommonProviderConfig {
            name: name.into(),
            upstream_url: String::new(),
            working_dir: path("data"),
            log_dir: path("log"),
            log_file: path("log/latest.log"),
            interval: Duration::from_secs(3600),
            retry: 1,
            timeout: Duration::from_secs(60),
            env: HashMap::new(),
            is_master: true,
            success_exit_codes: vec![],
            runner: Runner::Cgroup(Arc::clone(cg), mem),
        },
        command: cmd.into(),
        fail_on_match: None,
        size_pattern: None,
    })
    .unwrap()
}

#[tokio::test]
#[ignore = "needs root"]
async fn group_gets_limit_and_is_removed() {
    let Some((cg, _parent)) = setup("hustsync-test-limit") else {
        return;
    };
    let dir = cg.create("probe", Some(MemBytes(64 << 20))).await.unwrap();
    let file = match cg.version() {
        Version::V2 => "memory.max",
        Version::V1 => "memory.limit_in_bytes",
    };
    let got = std::fs::read_to_string(dir.join(file)).unwrap();
    assert_eq!(got.trim(), "67108864");
    assert!(!cg.finish("probe").await);
    assert!(!dir.exists());
}

#[tokio::test]
#[ignore = "needs root"]
async fn exceeding_memory_limit_reports_oom() {
    let Some((cg, _parent)) = setup("hustsync-test-oom") else {
        return;
    };
    let tmp = TempDir::new().unwrap();
    let p = provider(&tmp, "hog", "tail /dev/zero", &cg, Some(MemBytes(32 << 20)));
    let res = p.run(RunContext::default()).await;
    assert!(matches!(res, Err(ProviderError::OutOfMemory)), "{res:?}");
    assert!(!cg.dir("hog").exists());
}

#[tokio::test]
#[ignore = "needs root"]
async fn detached_children_are_killed() {
    let Some((cg, _parent)) = setup("hustsync-test-kill") else {
        return;
    };
    let tmp = TempDir::new().unwrap();
    let (pid_file, cg_file) = (tmp.path().join("pid"), tmp.path().join("cgroup"));
    let cmd = format!(
        "sh -c 'cat /proc/self/cgroup > {}; setsid sleep 300 & echo $! > {}'",
        cg_file.display(),
        pid_file.display()
    );
    let p = provider(&tmp, "daemon", &cmd, &cg, None);
    p.run(RunContext::default()).await.unwrap();

    let membership = std::fs::read_to_string(&cg_file).unwrap();
    assert!(
        membership.contains("/hustsync-test-kill/daemon"),
        "{membership}"
    );
    let pid = std::fs::read_to_string(&pid_file).unwrap();
    assert!(
        !Path::new("/proc").join(pid.trim()).exists(),
        "daemon {pid} survived"
    );
    assert!(!cg.dir("daemon").exists());
}
