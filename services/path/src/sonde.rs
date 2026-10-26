//! SPDX-License-Identifier: Apache-2.0
//! Live ionosonde readings (Lowell GIRO, via prop.kc2g.com) for the
//! planner's "live check". KC2G's stations file sends no CORS header, so the
//! page asks here; the file is fetched at most every 15 minutes.

use crate::geo::distance_km;
use chrono::{DateTime, NaiveDateTime, Utc};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

const UA: &str = "WeeChatRadioPath/0.1 (https://weechatradio.com; path helper)";
pub const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
/// A reading this old or a confidence score below this is not used.
const MAX_AGE_MIN: i64 = 60;
const MIN_CS: f64 = 50.0;
/// Past this a station says little about the path.
const MAX_KM: f64 = 3000.0;

#[derive(Debug, Clone, Serialize)]
pub struct Station {
    pub name: String,
    pub code: String,
    pub lat: f64,
    pub lon: f64,
    pub fof2_mhz: f64,
    pub mufd_mhz: f64,
    pub cs: f64,
    #[serde(serialize_with = "rfc3339")]
    pub time: DateTime<Utc>,
}

fn rfc3339<S: serde::Serializer>(t: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&t.to_rfc3339())
}

#[derive(Debug, Clone, Serialize)]
pub struct Nearest {
    #[serde(flatten)]
    pub station: Station,
    pub distance_km: u32,
    pub age_min: i64,
}

pub struct Sonde {
    client: reqwest::Client,
    url: String,
    cached: Mutex<Option<(Instant, DateTime<Utc>, Arc<Vec<Station>>)>>,
}

impl Sonde {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(8))
            .build()
            .expect("reqwest client");
        let url = std::env::var("WCR_PATH_STATIONS_URL")
            .unwrap_or_else(|_| "https://prop.kc2g.com/api/stations.json".into());
        Self {
            client,
            url,
            cached: Mutex::new(None),
        }
    }

    /// Every station in the file, and when the file was fetched.
    pub async fn stations(&self) -> Result<(DateTime<Utc>, Arc<Vec<Station>>), String> {
        if let Some((at, fetched, list)) = &*self.cached.lock() {
            if at.elapsed() < CACHE_TTL {
                return Ok((*fetched, list.clone()));
            }
        }
        let text = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        let list = Arc::new(parse_stations(&text)?);
        let fetched = Utc::now();
        *self.cached.lock() = Some((Instant::now(), fetched, list.clone()));
        Ok((fetched, list))
    }
}

/// The closest station to (`lat`, `lon`) with a recent, confident reading,
/// within 3000 km.
pub fn nearest(list: &[Station], lat: f64, lon: f64, now: DateTime<Utc>) -> Option<Nearest> {
    list.iter()
        .filter(|s| s.cs >= MIN_CS)
        .map(|s| {
            let age = (now - s.time).num_minutes();
            (s, age, distance_km((lat, lon), (s.lat, s.lon)))
        })
        .filter(|(_, age, km)| (-5..=MAX_AGE_MIN).contains(age) && *km <= MAX_KM)
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(s, age, km)| Nearest {
            station: s.clone(),
            distance_km: km.round() as u32,
            age_min: age.max(0),
        })
}

/// A number the file may write as 3.4 or "3.4".
fn number(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|x: &f64| x.is_finite())
}

/// "2026-10-26 20:15:00" (UTC) or RFC 3339.
fn time(v: Option<&Value>) -> Option<DateTime<Utc>> {
    let s = v?.as_str()?.trim();
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Some(t.with_timezone(&Utc));
    }
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
        .map(|t| t.and_utc())
}

fn parse_stations(text: &str) -> Result<Vec<Station>, String> {
    let arr: Vec<Value> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    Ok(arr
        .iter()
        .filter_map(|r| {
            let st = r.get("station")?;
            let lon = number(st.get("longitude"))?;
            Some(Station {
                name: st.get("name")?.as_str()?.trim().to_string(),
                code: st
                    .get("code")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string(),
                lat: number(st.get("latitude"))?,
                // The file counts longitude 0 to 360 east.
                lon: if lon > 180.0 { lon - 360.0 } else { lon },
                fof2_mhz: number(r.get("fof2"))?,
                mufd_mhz: number(r.get("mufd"))?,
                cs: number(r.get("cs")).unwrap_or(0.0),
                time: time(r.get("time"))?,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {"station":{"name":"Dourbes","code":"DB049","latitude":"50.1","longitude":"4.6"},
       "time":"2026-10-26 20:14:00","cs":75,"fof2":3.4,"mufd":10.1},
      {"station":{"name":"Chilton","code":"RL052","latitude":"51.5","longitude":"359.4"},
       "time":"2024-08-22 12:00:00","cs":0,"fof2":5.0,"mufd":15.0},
      {"station":{"name":"Fairford","code":"FF051","latitude":"51.7","longitude":"358.2"},
       "time":"2026-10-26 16:15:00","cs":90,"fof2":4.0,"mufd":12.0},
      {"station":{"name":"Rome","code":"RO041","latitude":"41.9","longitude":"12.5"},
       "time":"2026-10-26T20:00:00Z","cs":"60","fof2":"4.1","mufd":"12.5"},
      {"station":{"name":"Broken","latitude":"1","longitude":"1"},"time":"x","fof2":1,"mufd":1}
    ]"#;

    #[test]
    fn parses_numbers_and_strings_and_east_longitude() {
        let list = parse_stations(SAMPLE).unwrap();
        assert_eq!(list.len(), 4);
        assert!((list[1].lon - -0.6).abs() < 1e-9);
        assert!((list[3].mufd_mhz - 12.5).abs() < 1e-9);
    }

    #[test]
    fn stale_and_unsure_stations_are_skipped() {
        let list = parse_stations(SAMPLE).unwrap();
        let now = time(Some(&Value::from("2026-10-26 20:20:00"))).unwrap();
        // From Bath: Chilton (stale, cs 0) and Fairford (4 h old) are
        // nearer, but Dourbes is the nearest usable one.
        let n = nearest(&list, 51.38, -2.36, now).unwrap();
        assert_eq!(n.station.name, "Dourbes");
        assert_eq!(n.age_min, 6);
        assert!(
            n.distance_km > 400 && n.distance_km < 600,
            "{}",
            n.distance_km
        );
        // Nothing usable within 3000 km of New York.
        assert!(nearest(&list, 40.7, -74.0, now).is_none());
    }
}
