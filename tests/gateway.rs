//! Gateway detector loop.

use std::{
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

use localproxy::{config, gateway};

#[tokio::test]
async fn the_detector_returns_immediately_when_already_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let state = localproxy::testing::state(
        localproxy::testing::paths(dir.path()),
        config::AppConfig::default(),
    );
    state.shutdown.cancel();

    gateway::run(state).await.unwrap();
}

#[tokio::test]
async fn the_detector_clears_the_gateway_outside_gateway_mode() {
    let dir = tempfile::tempdir().unwrap();
    let state = localproxy::testing::state(
        localproxy::testing::paths(dir.path()),
        config::AppConfig::default(),
    );
    *state.gateway_ip.write().await = Some(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));

    let task_state = state.clone();
    let handle = tokio::spawn(async move { gateway::run(task_state).await });

    for _ in 0..200 {
        if state.gateway_ip.read().await.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(state.gateway_ip.read().await.is_none());

    state.shutdown.cancel();
    handle.await.unwrap().unwrap();
}
