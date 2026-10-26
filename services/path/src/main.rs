//! SPDX-License-Identifier: Apache-2.0
//! Path helper: which calling frequency for two map pins.

mod choose;
mod forecast;
mod geo;
mod places;
mod sonde;
mod ssn;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use choose::{
    band_of, far_fm, hf_bands, hf_freqs_mhz, pick_hf, pick_hf_band, try_close_fm, BandWorks, Card,
    HfAntenna, DEFAULT_ANTENNA_M, POWER_STEPS,
};
use chrono::{Datelike, Timelike, Utc};
use forecast::Forecast;
use geo::{distance_km, Region};
use places::{Place, Places};
use serde::Deserialize;
use sonde::Sonde;
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
    sonde: Arc<Sonde>,
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
    /// Antenna height above ground at both ends, in metres. Missing means
    /// 10 m. Older pages send only this.
    #[serde(default)]
    antenna_m: Option<f64>,
    /// TX and RX antenna heights in metres. Each falls back to `antenna_m`.
    #[serde(default)]
    tx_antenna_m: Option<f64>,
    #[serde(default)]
    rx_antenna_m: Option<f64>,
    /// HF antenna at the user's end. Missing means a dipole.
    #[serde(default)]
    hf_antenna: HfAntenna,
    /// HF band the user picked by hand ("40m"). The answer then also
    /// carries `band_card`, the prediction for that band alone, unless the
    /// band has no calling spot on this path.
    #[serde(default)]
    band: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlacesQuery {
    q: String,
}

#[derive(Debug, Deserialize)]
struct PointQuery {
    lat: f64,
    lon: f64,
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
        sonde: Arc::new(Sonde::new()),
    };

    let app = Router::new()
        .route("/api/path", post(path_handler))
        .route("/api/path/places", get(places_handler))
        .route("/api/path/ionosonde", get(ionosonde_handler))
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

/// The nearest ionosonde with a recent, confident reading to a point (the
/// page sends the path midpoint). `station` is null when none is usable.
/// `fetched_at` says when the stations file was last fetched; it is kept
/// for `cached_for_s`.
async fn ionosonde_handler(
    State(app): State<App>,
    Query(q): Query<PointQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if !(-90.0..=90.0).contains(&q.lat) || !(-180.0..=180.0).contains(&q.lon) {
        return Err((StatusCode::BAD_REQUEST, "bad coordinates".into()));
    }
    let (fetched, list) = app
        .sonde
        .stations()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    let station = sonde::nearest(&list, q.lat, q.lon, Utc::now());
    Ok(Json(serde_json::json!({
        "station": station,
        "fetched_at": fetched.to_rfc3339(),
        "cached_for_s": sonde::CACHE_TTL.as_secs(),
    })))
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
    let both_m = body.antenna_m.unwrap_or(DEFAULT_ANTENNA_M);
    let tx_m = body.tx_antenna_m.unwrap_or(both_m);
    let rx_m = body.rx_antenna_m.unwrap_or(both_m);
    if ![both_m, tx_m, rx_m]
        .iter()
        .all(|m| (0.5..=100.0).contains(m))
    {
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

    let HfAnswer {
        card: hf,
        band_card,
        bands,
        sky,
    } = hf_cards(
        &app,
        tx,
        rx,
        tx_r,
        rx_r,
        dist,
        body.hour_utc,
        max_w,
        body.hf_antenna,
        body.band.as_deref(),
    )
    .await?;
    if let Some(fm) = try_close_fm(tx_r, rx_r, dist, tx_m, rx_m) {
        return Ok(Json(Answer {
            card: fm,
            hf: Some(hf),
            fm: None,
            band_card,
            bands,
            sky,
        }));
    }
    Ok(Json(Answer {
        card: hf,
        hf: None,
        fm: far_fm(tx_r, rx_r, dist, tx_m, rx_m),
        band_card,
        bands,
        sky,
    }))
}

#[derive(serde::Serialize)]
struct Answer {
    #[serde(flatten)]
    card: Card,
    /// HF alternative when the flat-earth answer is local FM. The page
    /// switches to this if terrain blocks VHF and UHF.
    #[serde(skip_serializing_if = "Option::is_none")]
    hf: Option<Card>,
    /// 2m FM alternative when the answer is HF only because the path is past
    /// the smooth-earth horizon. The page switches to this when the real
    /// terrain leaves a radio line (only blocked paths go to HF).
    #[serde(skip_serializing_if = "Option::is_none")]
    fm: Option<Card>,
    /// The prediction for the HF band the user picked by hand.
    #[serde(skip_serializing_if = "Option::is_none")]
    band_card: Option<Card>,
    /// Every HF band on this path and whether it works at this hour and
    /// power. The page lists the ones that do on every card.
    bands: Vec<BandWorks>,
    sky: Sky,
}

/// The engine's figures for the sky view at the planned hour.
#[derive(serde::Serialize)]
struct Sky {
    /// Path MUF in MHz, from VOACAP.
    muf_mhz: Option<f64>,
    /// Hops of the mode at the MUF.
    muf_hops: Option<u8>,
    /// False when NOAA's sunspot number was unreachable and a typical
    /// month (80) was assumed.
    solar_live: bool,
}

struct HfAnswer {
    card: Card,
    band_card: Option<Card>,
    bands: Vec<BandWorks>,
    sky: Sky,
}

const SOLAR_FALLBACK_NOTE: &str = "Solar data unavailable, typical month assumed.";

/// The HF pick, the pick for `band` alone when the user chose one, which
/// HF bands work on the path, and the sky view's figures.
#[allow(clippy::too_many_arguments)]
async fn hf_cards(
    app: &App,
    tx: (f64, f64),
    rx: (f64, f64),
    tx_r: Region,
    rx_r: Region,
    dist: f64,
    hour_utc: Option<u8>,
    max_w: u32,
    antenna: HfAntenna,
    band: Option<&str>,
) -> Result<HfAnswer, (StatusCode, String)> {
    let now = Utc::now();
    let this_hour = now.hour() as u8;
    let hour = hour_utc.unwrap_or(this_hour);
    let freqs = hf_freqs_mhz(tx_r, rx_r);
    // A band with no calling spot here (40m outside a shared plan) gets no
    // band_card; the page says so.
    let band = band.filter(|b| freqs.iter().any(|&f| band_of(f) == Some(*b)));
    let (ssn, solar_live) = app.ssn.current().await;
    let forecast = app.forecast.clone();
    let year = now.year();
    let month = now.month();
    let predict_freqs = freqs.clone();
    let prediction = tokio::task::spawn_blocking(move || {
        forecast.predict(tx, rx, &predict_freqs, ssn, year, month)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;

    let mut cells = prediction.cells;
    antenna.apply(&mut cells);
    let now = hour == this_hour;
    let solar = |c: &mut Card| {
        antenna.footnote(c);
        if !solar_live {
            c.footnote = format!("{SOLAR_FALLBACK_NOTE} {}", c.footnote);
        }
    };
    let mut card = pick_hf(&cells, hour, now, dist, max_w);
    solar(&mut card);
    let band_card = band.map(|b| {
        let only: Vec<_> = cells
            .iter()
            .copied()
            .filter(|c| band_of(c.freq_mhz) == Some(b))
            .collect();
        let mut c = pick_hf_band(&only, b, hour, now, dist, max_w);
        solar(&mut c);
        c
    });
    let bands = hf_bands(&cells, &freqs, hour, max_w, &prediction.muf_by_hour);
    let h = hour as usize % 24;
    let muf = prediction.muf_by_hour[h];
    Ok(HfAnswer {
        card,
        band_card,
        bands,
        sky: Sky {
            muf_mhz: (muf > 0.0).then_some(muf),
            muf_hops: prediction.muf_hops_by_hour[h],
            solar_live,
        },
    })
}
