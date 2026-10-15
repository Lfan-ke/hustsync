//! `GET /metrics` on the worker control server.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt as _;
use hustsync_worker::{
    schedule::ScheduleQueue,
    server::{AppState, make_http_server},
};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tower::ServiceExt;

#[tokio::test]
async fn metrics_report_jobs_queue_and_concurrency() {
    let semaphore = Arc::new(Semaphore::new(3));
    let _held = Arc::clone(&semaphore).acquire_owned().await.unwrap();
    let state = Arc::new(AppState {
        jobs: Arc::new(RwLock::new(HashMap::new())),
        schedule_queue: Arc::new(Mutex::new(ScheduleQueue::new())),
        semaphore,
        concurrent: 3,
        reload_tx: None,
    });

    let req = Request::get("/metrics").body(Body::empty()).unwrap();
    let resp = make_http_server(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    for line in [
        "hustsync_worker_jobs 0",
        "hustsync_worker_jobs_running 1",
        "hustsync_worker_jobs_queued 0",
        "hustsync_worker_concurrency 3",
    ] {
        assert!(
            text.lines().any(|l| l == line),
            "missing `{line}` in
{text}"
        );
    }
}
