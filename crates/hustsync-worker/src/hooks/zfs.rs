//! `[zfs]`: refuses to sync into a directory that is not a ZFS dataset of its own,
//! so a missing dataset cannot fill the parent with one mirror's data.

use std::io;

use async_trait::async_trait;

use super::{HookCtx, HookError, JobHook, mount};

const HOOK_NAME: &str = "zfs";

pub struct ZfsHook {
    pool: String,
}

impl ZfsHook {
    pub fn new(pool: impl Into<String>) -> Self {
        Self { pool: pool.into() }
    }

    fn help(&self, ctx: &HookCtx) -> String {
        let dataset = format!("{}/{}", self.pool, ctx.mirror_name).to_lowercase();
        let dir = ctx.working_dir.display();
        let uid = nix::unistd::geteuid();
        let own = if uid.is_root() {
            String::new()
        } else {
            format!(" && chown {uid} '{dir}'")
        };
        format!(
            "create it with: zfs create '{dataset}' && zfs set mountpoint='{dir}' '{dataset}'{own}"
        )
    }
}

#[async_trait]
impl JobHook for ZfsHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    async fn pre_job(&self, ctx: &mut HookCtx) -> Result<(), HookError> {
        let dir = ctx.working_dir.clone();
        let found = tokio::task::spawn_blocking(move || mount::fs_type(&dir))
            .await
            .map_err(|e| HookError::io(HOOK_NAME, io::Error::other(e)))?;
        let why = match found {
            Ok(Some(fs)) if fs == "zfs" => return Ok(()),
            Ok(Some(fs)) => format!("is a {fs} mount, not a ZFS dataset"),
            Ok(None) => "is not a mount point".to_owned(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => "does not exist".to_owned(),
            Err(e) => return Err(HookError::io(HOOK_NAME, e)),
        };
        Err(HookError::config(
            HOOK_NAME,
            format!("{} {why}; {}", ctx.working_dir.display(), self.help(ctx)),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use super::*;

    fn ctx(dir: impl Into<PathBuf>) -> HookCtx {
        HookCtx {
            mirror_name: "Debian".into(),
            working_dir: dir.into(),
            upstream_url: String::new(),
            log_dir: PathBuf::new(),
            log_file: PathBuf::new(),
            attempt: 0,
            env: HashMap::new(),
        }
    }

    async fn refusal(dir: impl Into<PathBuf>) -> String {
        let err = ZfsHook::new("tank/mirrors")
            .pre_job(&mut ctx(dir))
            .await
            .expect_err("must refuse");
        err.to_string()
    }

    #[tokio::test]
    async fn refuses_a_plain_directory_and_says_how_to_fix_it() {
        let tmp = tempfile::tempdir().unwrap();
        let msg = refusal(tmp.path()).await;
        assert!(msg.contains("is not a mount point"), "{msg}");
        assert!(msg.contains("zfs create 'tank/mirrors/debian'"), "{msg}");
        assert!(msg.contains("zfs set mountpoint="), "{msg}");
    }

    #[tokio::test]
    async fn refuses_another_filesystem() {
        let msg = refusal("/proc").await;
        assert!(msg.contains("is a proc mount, not a ZFS dataset"), "{msg}");
    }

    #[tokio::test]
    async fn refuses_a_missing_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let msg = refusal(tmp.path().join("gone")).await;
        assert!(msg.contains("does not exist"), "{msg}");
    }
}
