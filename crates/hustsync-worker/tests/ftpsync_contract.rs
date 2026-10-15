//! An ftpsync mirror against a local rsync daemon; skipped without ftpsync or rsync.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    net::TcpListener,
    process::{Child, Command},
    time::Duration,
};

use hustsync_config_parser::{MirrorConfig, WorkerConfig};
use hustsync_worker::provider::{ProviderType, RunContext, build_provider};
use tempfile::TempDir;

fn has(bin: &str) -> bool {
    let ok = Command::new("sh")
        .args(["-c", &format!("command -v {bin}")])
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("skipped: {bin} not installed");
    }
    ok
}

/// An rsync daemon serving `dir` as module `debian`; killed on drop.
struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn serve(tmp: &TempDir) -> (Daemon, u16) {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let conf = tmp.path().join("rsyncd.conf");
    std::fs::write(
        &conf,
        format!(
            "port = {port}\naddress = 127.0.0.1\nuse chroot = no\n[debian]\npath = {}\nread only = yes\n",
            tmp.path().join("src").display()
        ),
    )
    .unwrap();
    let child = Command::new("rsync")
        .args([
            "--daemon",
            "--no-detach",
            &format!("--config={}", conf.display()),
        ])
        .spawn()
        .unwrap();
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    (Daemon(child), port)
}

fn seed(tmp: &TempDir) {
    let src = tmp.path().join("src");
    for (file, body) in [
        ("pool/main/h/hello/hello_1.0_amd64.deb", "deb"),
        ("dists/sid/Release", "Suite: sid\n"),
        ("project/trace/master", "Archive serial: 2026101601\n"),
    ] {
        let p = src.join(file);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
}

#[tokio::test]
async fn ftpsync_mirrors_an_archive() {
    if !has("ftpsync") || !has("rsync") {
        return;
    }
    let tmp = TempDir::new().unwrap();
    seed(&tmp);
    let (_daemon, port) = serve(&tmp);
    let dir = |p: &str| tmp.path().join(p).to_string_lossy().into_owned();
    let m = MirrorConfig {
        name: Some("debian".into()),
        provider: Some("ftpsync".into()),
        upstream: Some("rsync://127.0.0.1/debian/".into()),
        mirror_dir: Some(dir("dst")),
        log_dir: Some(dir("log")),
        ftpsync_conf: Some(vec![
            format!("RSYNC_EXTRA='--port={port}'"),
            "MIRRORNAME='test.mirror'".into(),
        ]),
        ..MirrorConfig::default()
    };
    let p = build_provider("debian", &m, &WorkerConfig::default(), None).unwrap();
    assert_eq!(p.provider_type(), ProviderType::Ftpsync);

    p.run(RunContext::default()).await.unwrap();

    let dst = tmp.path().join("dst");
    assert!(dst.join("pool/main/h/hello/hello_1.0_amd64.deb").exists());
    assert!(dst.join("dists/sid/Release").exists());
    assert!(
        dst.join("project/trace/test.mirror").exists(),
        "ftpsync wrote no trace"
    );
}
