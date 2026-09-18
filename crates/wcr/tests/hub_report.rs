//! Signed POST /api/v1/report against a local hub listener.

use axum::routing::post;
use axum::Router;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use wcr::net::hub_server;
use wcr::proto::IdentityKeys;
use wcr::telemetry::{self, NodeReport};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn spawn_report_server(state: hub_server::HubState) -> SocketAddr {
    let app = Router::new()
        .route("/api/v1/report", post(telemetry::ingest_report))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    addr
}

fn sample_report() -> NodeReport {
    NodeReport {
        callsign: "G4ABC".into(),
        ts: now(),
        grid: "IO91WM".into(),
        mode: "internet-radio".into(),
        ptt: "none".into(),
        preset: "vhf-fm".into(),
        snr: 10.0,
        ber: 0.0,
        queue: 0,
        hub_ok: true,
        settings: serde_json::json!({}),
        events: vec![],
        freq_khz: 144950,
        band: "2m".into(),
    }
}

#[tokio::test]
async fn ingest_signed_report() {
    let keys = IdentityKeys::generate();
    let addr = spawn_report_server(hub_server::test_hub_state()).await;
    let report = sample_report();
    let body = serde_json::to_vec(&report).unwrap();
    let sig = keys.sign_bytes(&body);

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/api/v1/report"))
        .header("X-Radio-Callsign", "G4ABC")
        .header("X-Radio-Pubkey", keys.public_hex())
        .header("X-Radio-Signature", hex::encode(sig))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let v: serde_json::Value = res.json().await.unwrap();
    assert_eq!(v.get("ok"), Some(&serde_json::Value::Bool(true)));
}

#[tokio::test]
async fn ingest_rejects_blocked_pubkey() {
    let keys = IdentityKeys::generate();
    let pk = keys.public_bytes();
    let state = hub_server::test_hub_state();
    state.telemetry.block_pubkey(&pk).unwrap();
    let addr = spawn_report_server(state).await;

    let report = sample_report();
    let body = serde_json::to_vec(&report).unwrap();
    let sig = keys.sign_bytes(&body);

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/api/v1/report"))
        .header("X-Radio-Callsign", "G4ABC")
        .header("X-Radio-Pubkey", keys.public_hex())
        .header("X-Radio-Signature", hex::encode(sig))
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::FORBIDDEN);
}
