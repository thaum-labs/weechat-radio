//! SPDX-License-Identifier: Apache-2.0
//! Path helper: which calling frequency for two map pins.

mod choose;
mod forecast;
mod geo;
mod places;
mod ssn;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use choose::{hf_freqs_mhz, pick_hf, try_close_fm, Card};
use chrono::{Datelike, Timelike, Utc};
use forecast::Forecast;
use geo::{distance_km, Region};
use places::{Place, Places};
use serde::Deserialize;
use ssn::Ssn;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
struct App {
    places: Arc<Places>,
    ssn: Arc<Ssn>,
    forecast: Arc<Forecast>,
}

#[derive(Debug, Deserialize)]
struct PathBody {
    tx_lat: f64,
    tx_lon: f64,
    rx_lat: f64,
    rx_lon: f64,
}

#[derive(Debug, Deserialize)]
struct PlacesQuery {
    q: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wcr_path=info,tower_http=info".into()),
        )
        .init();

    let app_state = App {
        places: Arc::new(Places::new()),
        ssn: Arc::new(Ssn::new()),
        forecast: Arc::new(Forecast::new()),
    };

    let app = Router::new()
        .route("/api/path", post(path_handler))
        .route("/api/path/places", get(places_handler))
        .route("/healthz", get(|| async { "ok" }))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);

    let bind = std::env::var("WCR_PATH_BIND").unwrap_or_else(|_| "0.0.0.0:7380".into());
    let addr: SocketAddr = bind.parse().expect("WCR_PATH_BIND");
    tracing::info!("wcr-path listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

async fn places_handler(
    State(app): State<App>,
    Query(q): Query<PlacesQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let hits = app
        .places
        .search(&q.q)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(serde_json::json!({ "places": hits })))
}

async fn path_handler(
    State(app): State<App>,
    Json(body): Json<PathBody>,
) -> Result<Json<Card>, (StatusCode, String)> {
    if !body.tx_lat.is_finite()
        || !body.tx_lon.is_finite()
        || !body.rx_lat.is_finite()
        || !body.rx_lon.is_finite()
        || !(-90.0..=90.0).contains(&body.tx_lat)
        || !(-90.0..=90.0).contains(&body.rx_lat)
        || !(-180.0..=180.0).contains(&body.tx_lon)
        || !(-180.0..=180.0).contains(&body.rx_lon)
    {
        return Err((StatusCode::BAD_REQUEST, "bad coordinates".into()));
    }

    let tx = (body.tx_lat, body.tx_lon);
    let rx = (body.rx_lat, body.rx_lon);
    let dist = distance_km(tx, rx);

    let (tx_place, rx_place) = tokio::join!(
        app.places.reverse(tx.0, tx.1),
        app.places.reverse(rx.0, rx.1)
    );
    let tx_place = tx_place.unwrap_or(Place {
        label: String::new(),
        lat: tx.0,
        lon: tx.1,
        country: String::new(),
    });
    let rx_place = rx_place.unwrap_or(Place {
        label: String::new(),
        lat: rx.0,
        lon: rx.1,
        country: String::new(),
    });
    let tx_r = Region::from_country(&tx_place.country);
    let rx_r = Region::from_country(&rx_place.country);

    if let Some(card) = try_close_fm(tx_r, rx_r, dist) {
        return Ok(Json(card));
    }

    let now = Utc::now();
    let ssn = app.ssn.current().await;
    let freqs = hf_freqs_mhz(tx_r, rx_r);
    let forecast = app.forecast.clone();
    let year = now.year();
    let month = now.month();
    let cells =
        tokio::task::spawn_blocking(move || forecast.predict(tx, rx, &freqs, ssn, year, month))
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;

    let hour = now.hour() as u8;
    Ok(Json(pick_hf(&cells, hour, dist)))
}
