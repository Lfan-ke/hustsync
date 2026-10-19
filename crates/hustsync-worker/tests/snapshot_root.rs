//! The ZFS and Btrfs hooks on real filesystems. Needs a Btrfs mount and a ZFS dataset's mount point:
//! `HUSTSYNC_BTRFS=<dir> HUSTSYNC_ZFS=<dir> sudo -E <test binary> --ignored --test-threads=1`.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::HashMap,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use hustsync_worker::hooks::{BtrfsSnapshotHook, HookCtx, JobHook, ZfsHook, btrfs::subvol};

fn ctx(name: &str, dir: &Path) -> HookCtx {
    HookCtx {
        mirror_name: name.into(),
        working_dir: dir.into(),
        upstream_url: String::new(),
        log_dir: PathBuf::new(),
        log_file: PathBuf::new(),
        attempt: 0,
        env: HashMap::new(),
    }
}

fn scratch(var: &str) -> Option<PathBuf> {
    let dir = std::env::var_os(var).map(PathBuf::from);
    if dir.is_none() {
        eprintln!("skipped: {var} is not set");
    }
    dir
}

#[tokio::test]
#[ignore = "needs a Btrfs mount in HUSTSYNC_BTRFS"]
async fn btrfs_creates_the_subvolume_and_swaps_in_each_snapshot() {
    let Some(base) = scratch("HUSTSYNC_BTRFS") else {
        return;
    };
    let work = base.join("debian");
    let publish = base.join("publish");
    let snap = publish.join("debian");
    fs::create_dir_all(&publish).unwrap();
    let hook = BtrfsSnapshotHook::new(&snap);
    let mut c = ctx("debian", &work);

    hook.pre_job(&mut c).await.unwrap();
    assert_eq!(
        fs::metadata(&work).unwrap().ino(),
        256,
        "working dir must be a subvolume"
    );
    hook.pre_job(&mut c).await.unwrap();

    fs::write(work.join("Release"), "1").unwrap();
    hook.post_success(&mut c).await.unwrap();
    assert_eq!(fs::read_to_string(snap.join("Release")).unwrap(), "1");

    fs::write(work.join("Release"), "2").unwrap();
    fs::write(work.join("InRelease"), "2").unwrap();
    hook.post_success(&mut c).await.unwrap();
    assert_eq!(fs::read_to_string(snap.join("Release")).unwrap(), "2");
    assert!(snap.join("InRelease").exists());
    assert!(
        !publish.join(".debian.new").exists(),
        "the old snapshot must be gone"
    );
    assert!(subvol::is_subvolume(&snap).unwrap());

    hook.post_fail(&mut c).await.unwrap();
    assert_eq!(fs::read_to_string(snap.join("Release")).unwrap(), "2");

    subvol::delete(&snap).unwrap();
    subvol::delete(&work).unwrap();
    fs::remove_dir(&publish).unwrap();
}

#[tokio::test]
#[ignore = "needs a Btrfs mount in HUSTSYNC_BTRFS"]
async fn btrfs_refuses_plain_directories() {
    let Some(base) = scratch("HUSTSYNC_BTRFS") else {
        return;
    };
    let plain = base.join("plain");
    fs::create_dir_all(&plain).unwrap();
    let err = BtrfsSnapshotHook::new(base.join("snap"))
        .pre_job(&mut ctx("plain", &plain))
        .await
        .expect_err("a plain directory is no subvolume");
    assert!(
        err.to_string().contains("is not a Btrfs subvolume"),
        "{err}"
    );

    let work = base.join("arch");
    let mut c = ctx("arch", &work);
    let hook = BtrfsSnapshotHook::new(&plain);
    hook.pre_job(&mut c).await.unwrap();
    let err = hook
        .post_success(&mut c)
        .await
        .expect_err("a plain directory must not be replaced");
    assert!(err.to_string().contains("is not a Btrfs snapshot"), "{err}");
    assert!(!base.join(".plain.new").exists());

    subvol::delete(&work).unwrap();
    fs::remove_dir(&plain).unwrap();
}

#[tokio::test]
#[ignore = "needs a ZFS dataset mounted at HUSTSYNC_ZFS"]
async fn zfs_accepts_a_dataset_and_refuses_a_directory_inside_it() {
    let Some(dataset) = scratch("HUSTSYNC_ZFS") else {
        return;
    };
    let hook = ZfsHook::new("hstest");
    hook.pre_job(&mut ctx("debian", &dataset)).await.unwrap();

    let inner = dataset.join("inner");
    fs::create_dir_all(&inner).unwrap();
    let err = hook
        .pre_job(&mut ctx("inner", &inner))
        .await
        .expect_err("a directory inside a dataset is not a dataset");
    assert!(err.to_string().contains("is not a mount point"), "{err}");
    fs::remove_dir(&inner).unwrap();
}
