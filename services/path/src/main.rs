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
use choose::{hf_freqs_mhz, pick_hf, try_close_fm, Card, DEFAULT_ANTENNA_M, POWER_STEPS};
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
    /// Hour to plan for (0-23 UTC). Missing means this hour.
    #[serde(default)]
    hour_utc: Option<u8>,
    /// Most power the station has, in watts. Missing means 100 W.
    #[serde(default)]
    power_w: Option<u32>,
    /// Antenna height above ground at each end, in metres. Missing means 10 m.
    #[serde(default)]
    antenna_m: Option<f64>,
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
) -> Result<Json<Answer>, (StatusCode, String)> {
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
    if body.hour_utc.is_some_and(|h| h > 23) {
        return Err((StatusCode::BAD_REQUEST, "bad hour".into()));
    }
    let max_w = body.power_w.unwrap_or(100);
    if !POWER_STEPS.contains(&max_w) {
        return Err((StatusCode::BAD_REQUEST, "bad power".into()));
    }
    let antenna_m = body.antenna_m.unwrap_or(DEFAULT_ANTENNA_M);
    if !(0.5..=100.0).contains(&antenna_m) {
        return Err((StatusCode::BAD_REQUEST, "bad antenna height".into()));
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

    let hf = hf_card(&app, tx, rx, tx_r, rx_r, dist, body.hour_utc, max_w).await?;
    if let Some(fm) = try_close_fm(tx_r, rx_r, dist, antenna_m) {
        return Ok(Json(Answer {
            card: fm,
            hf: Some(hf),
        }));
    }
    Ok(Json(Answer { card: hf, hf: None }))
}

#[derive(serde::Serialize)]
struct Answer {
    #[serde(flatten)]
    card: Card,
    /// HF alternative when the flat-earth answer is local FM. The page
    /// switches to this if terrain blocks VHF and UHF.
    #[serde(skip_serializing_if = "Option::is_none")]
    hf: Option<Card>,
}

async fn hf_card(
    app: &App,
    tx: (f64, f64),
    rx: (f64, f64),
    tx_r: Region,
    rx_r: Region,
    dist: f64,
    hour_utc: Option<u8>,
    max_w: u32,
) -> Result<Card, (StatusCode, String)> {
    let now = Utc::now();
    let this_hour = now.hour() as u8;
    let hour = hour_utc.unwrap_or(this_hour);
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

    Ok(pick_hf(&cells, hour, hour == this_hour, dist, max_w))
}
