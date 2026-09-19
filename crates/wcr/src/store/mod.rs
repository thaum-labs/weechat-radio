//! SPDX-License-Identifier: Apache-2.0
//! SQLite store: messages, delivery, seen ids, groups, heard stations.
//! WAL + synchronous=FULL so power loss never drops an acknowledged message.

use crate::error::Result;
use crate::proto::{Callsign, Envelope, Flags, MsgId, MsgType};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: &str = include_str!("schema.sql");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Queued,
    Sent,
    Relayed,
    Delivered,
    All,
}

impl Delivery {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Sent => "sent",
            Self::Relayed => "relayed",
            Self::Delivered => "delivered",
            Self::All => "all",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "sent" => Self::Sent,
            "relayed" => Self::Relayed,
            "delivered" => Self::Delivered,
            "all" => Self::All,
            _ => Self::Queued,
        }
    }

    pub fn ticks_bracket(self) -> &'static str {
        match self {
            Self::Queued => "[..]",
            Self::Sent => "[tx]",
            Self::Relayed => "[rl]",
            Self::Delivered => "[ok]",
            Self::All => "[all]",
        }
    }

    pub fn ticks(self, unicode: bool) -> &'static str {
        if unicode {
            match self {
                Self::Queued => "·",
                Self::Sent => "✓",
                Self::Relayed => "✓✓",
                Self::Delivered => "✓✓",
                Self::All => "✓✓✓",
            }
        } else {
            match self {
                Self::Queued => "-",
                Self::Sent => "v",
                Self::Relayed => "vv",
                Self::Delivered => "VV",
                Self::All => "VVV",
            }
        }
    }

    pub fn highlight_delivered(self) -> bool {
        matches!(self, Self::Delivered | Self::All)
    }
}

/// First-hop receive path: radio decode (`rf`), hub (`inet`), or LAN mesh (`lan`).
pub fn normalize_via(medium: Option<&str>) -> Option<&'static str> {
    match medium.map(str::trim).unwrap_or("") {
        "rf" => Some("rf"),
        "lan" => Some("lan"),
        "inet" => Some("inet"),
        "" => None,
        _ => Some("inet"),
    }
}

pub fn via_bracket(medium: &str) -> &'static str {
    match normalize_via(Some(medium)) {
        Some("rf") => "[rf]",
        Some("lan") => "[lan]",
        Some("inet") => "[net]",
        _ => "",
    }
}

pub fn tries_bracket(n: u32) -> String {
    if n == 0 {
        String::new()
    } else {
        format!("[x{n}]")
    }
}

fn chat_when(raw: Option<&str>) -> chrono::DateTime<chrono::Local> {
    if let Some(s) = raw.filter(|s| !s.is_empty()) {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return dt.with_timezone(&chrono::Local);
        }
        if let Ok(secs) = s.parse::<i64>() {
            if let Some(dt) = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0) {
                return dt.with_timezone(&chrono::Local);
            }
        }
    }
    chrono::Local::now()
}

/// Local date + time to the second, e.g. `2026-09-19 22:46:03`.
pub fn chat_stamp(raw: Option<&str>) -> String {
    chat_when(raw).format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn via_hint(medium: &str) -> &'static str {
    match normalize_via(Some(medium)) {
        Some("rf") => "received by radio decode",
        Some("lan") => "received on the local network",
        Some("inet") => "received over the internet",
        _ => "",
    }
}

#[derive(Debug, Clone)]
pub struct StoredMsg {
    pub env: Envelope,
    pub rx_time: u32,
    pub delivery: Delivery,
}

#[derive(Debug, Clone)]
pub struct HeardStation {
    pub callsign: String,
    pub last_heard: u32,
    pub snr: Option<f32>,
    pub mode: Option<String>,
    pub grid: Option<String>,
    pub gateway: bool,
    pub medium: String,
    pub freq_khz: u32,
    pub band: Option<String>,
}

pub struct Store {
    conn: Mutex<Connection>,
    max_age_hours: u64,
    max_msgs: u64,
}

impl Store {
    pub fn open(path: &Path, max_age_hours: u64, max_msgs: u64) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(SCHEMA)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        Self::migrate(&conn);
        let s = Self {
            conn: Mutex::new(conn),
            max_age_hours,
            max_msgs,
        };
        s.gc()?;
        Ok(s)
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Self::migrate(&conn);
        Ok(Self {
            conn: Mutex::new(conn),
            max_age_hours: 72,
            max_msgs: 10_000,
        })
    }

    fn migrate(conn: &Connection) {
        let _ = conn.execute(
            "ALTER TABLE heard ADD COLUMN freq_khz INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute("ALTER TABLE heard ADD COLUMN band TEXT", []);
        let _ = conn.execute(
            "ALTER TABLE groups ADD COLUMN default_prio INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN rf_tx INTEGER NOT NULL DEFAULT 0",
            [],
        );
    }

    fn now() -> u32 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as u32)
            .unwrap_or(0)
    }

    pub fn next_seq(&self, callsign: &str) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let next: u32 = conn
            .query_row(
                "SELECT next_seq FROM seq WHERE callsign = ?1",
                params![callsign],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(1);
        conn.execute(
            "INSERT INTO seq(callsign, next_seq) VALUES(?1, ?2)
             ON CONFLICT(callsign) DO UPDATE SET next_seq = excluded.next_seq",
            params![callsign, next + 1],
        )?;
        Ok(next)
    }

    pub fn insert(&self, env: &Envelope, delivery: Delivery) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let now = Self::now();
        let changed = conn.execute(
            "INSERT OR IGNORE INTO messages(
                msg_id, kind, origin, dest, flags, hops_left, ts, seq, body, signature,
                rx_time, delivery, is_group)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                env.msg_id.hex(),
                env.kind.as_str(),
                env.origin.to_string(),
                env.dest.to_string(),
                env.flags.raw() as i64,
                env.hops_left as i64,
                env.ts as i64,
                env.seq as i64,
                env.body.as_slice(),
                env.signature.as_ref().map(|s| s.as_slice()),
                now as i64,
                delivery.as_str(),
                env.flags.group() as i64,
            ],
        )?;
        conn.execute(
            "INSERT INTO seen(msg_id, first_seen, last_seen, hear_count)
             VALUES(?1,?2,?2,1)
             ON CONFLICT(msg_id) DO UPDATE SET last_seen = excluded.last_seen,
                hear_count = hear_count + 1",
            params![env.msg_id.hex(), now as i64],
        )?;
        Ok(changed > 0)
    }

    pub fn seen_before(&self, id: &MsgId) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM seen WHERE msg_id = ?1",
            params![id.hex()],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn hear_count(&self, id: &MsgId) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let n: u32 = conn
            .query_row(
                "SELECT hear_count FROM seen WHERE msg_id = ?1",
                params![id.hex()],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        Ok(n)
    }

    pub fn set_delivery(&self, id: &MsgId, d: Delivery) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET delivery = ?1 WHERE msg_id = ?2",
            params![d.as_str(), id.hex()],
        )?;
        Ok(())
    }

    pub fn delivery_of(&self, id: &MsgId) -> Result<Option<Delivery>> {
        let conn = self.conn.lock().unwrap();
        let s: Option<String> = conn
            .query_row(
                "SELECT delivery FROM messages WHERE msg_id = ?1",
                params![id.hex()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(s.map(|x| Delivery::parse(&x)))
    }

    pub fn get(&self, id: &MsgId) -> Result<Option<StoredMsg>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT kind, origin, dest, flags, hops_left, ts, seq, body, signature, rx_time, delivery
                 FROM messages WHERE msg_id = ?1",
                params![id.hex()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                        r.get::<_, i64>(6)?,
                        r.get::<_, Vec<u8>>(7)?,
                        r.get::<_, Option<Vec<u8>>>(8)?,
                        r.get::<_, i64>(9)?,
                        r.get::<_, String>(10)?,
                    ))
                },
            )
            .optional()?;
        Ok(row.map(|t| row_to_stored(id.clone(), t)))
    }

    pub fn history(&self, target: Option<&str>, limit: usize) -> Result<Vec<StoredMsg>> {
        let conn = self.conn.lock().unwrap();
        let mut out = Vec::new();
        if let Some(t) = target {
            let key = t
                .trim()
                .trim_start_matches('#')
                .trim_start_matches('&')
                .to_ascii_uppercase();
            let mut stmt = conn.prepare(
                "SELECT msg_id, kind, origin, dest, flags, hops_left, ts, seq, body, signature, rx_time, delivery
                 FROM messages WHERE upper(dest) = ?1 OR upper(origin) = ?1
                 ORDER BY rx_time ASC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![key, limit as i64], map_full_row)?;
            for r in rows {
                out.push(r?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT msg_id, kind, origin, dest, flags, hops_left, ts, seq, body, signature, rx_time, delivery
                 FROM messages ORDER BY rx_time ASC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit as i64], map_full_row)?;
            for r in rows {
                out.push(r?);
            }
        }
        Ok(out)
    }

    pub fn purge(&self, target: Option<&str>) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let n = if let Some(t) = target {
            conn.execute(
                "DELETE FROM messages WHERE dest = ?1 OR origin = ?1",
                params![t],
            )?
        } else {
            conn.execute("DELETE FROM messages", [])?
        };
        Ok(n as u64)
    }

    /// Drop this computer's copy of one channel or DM. Other stations keep theirs.
    pub fn purge_channel(&self, target: &str, our_call: Option<&str>) -> Result<u64> {
        let trimmed = target.trim();
        let group = trimmed.starts_with('#') || trimmed.starts_with('&');
        let key = trimmed
            .trim_start_matches('#')
            .trim_start_matches('&')
            .to_ascii_uppercase();
        if key.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().unwrap();
        let n = if group || key == "BULLETIN" || key == "BEACON" {
            conn.execute(
                "DELETE FROM messages WHERE is_group = 1 AND upper(dest) = ?1",
                params![key],
            )?
        } else if let Some(us) = our_call.filter(|s| !s.is_empty()) {
            let us = us.to_ascii_uppercase();
            conn.execute(
                "DELETE FROM messages WHERE is_group = 0 AND (
                    (upper(origin) = ?1 AND upper(dest) = ?2)
                    OR (upper(origin) = ?2 AND upper(dest) = ?1)
                )",
                params![us, key],
            )?
        } else {
            conn.execute(
                "DELETE FROM messages WHERE is_group = 0 AND (upper(dest) = ?1 OR upper(origin) = ?1)",
                params![key],
            )?
        };
        Ok(n as u64)
    }

    pub fn hold_due(&self, now: u32) -> Result<Vec<StoredMsg>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT msg_id, kind, origin, dest, flags, hops_left, ts, seq, body, signature, rx_time, delivery
             FROM messages WHERE hold_until > 0 AND hold_until <= ?1 AND suppressed = 0 AND hops_left > 0",
        )?;
        let rows = stmt.query_map(params![now as i64], map_full_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn set_hold(&self, id: &MsgId, until: u32, hops: u8) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET hold_until = ?1, hops_left = ?2 WHERE msg_id = ?3",
            params![until as i64, hops as i64, id.hex()],
        )?;
        Ok(())
    }

    pub fn suppress(&self, id: &MsgId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET suppressed = 1, hold_until = 0 WHERE msg_id = ?1",
            params![id.hex()],
        )?;
        Ok(())
    }

    pub fn bump_retry(&self, id: &MsgId, next_hold: u32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET retries = retries + 1, hold_until = ?1 WHERE msg_id = ?2",
            params![next_hold as i64, id.hex()],
        )?;
        Ok(())
    }

    pub fn bump_rf_tx(&self, id: &MsgId) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "UPDATE messages SET rf_tx = rf_tx + 1 WHERE msg_id = ?1",
            params![id.hex()],
        )?;
        if n == 0 {
            return Ok(0);
        }
        let v: i64 = conn.query_row(
            "SELECT rf_tx FROM messages WHERE msg_id = ?1",
            params![id.hex()],
            |r| r.get(0),
        )?;
        Ok(v as u32)
    }

    pub fn rf_tx_of(&self, id: &MsgId) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT rf_tx FROM messages WHERE msg_id = ?1",
                params![id.hex()],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0) as u32)
    }

    pub fn retries_of(&self, id: &MsgId) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT retries FROM messages WHERE msg_id = ?1",
                params![id.hex()],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0) as u32)
    }

    /// Our own unacked messages whose retry timer has fired.
    pub fn retry_due(
        &self,
        now: u32,
        origin: &str,
        max_retries: u32,
    ) -> Result<Vec<(StoredMsg, u32)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT msg_id, kind, origin, dest, flags, hops_left, ts, seq, body, signature, rx_time, delivery, retries
             FROM messages
             WHERE origin = ?1
               AND delivery = 'sent'
               AND hold_until > 0 AND hold_until <= ?2
               AND retries < ?3",
        )?;
        let rows = stmt.query_map(params![origin, now as i64, max_retries as i64], |r| {
            let hex: String = r.get(0)?;
            let id = MsgId::parse_hex(&hex).unwrap_or(MsgId([0; 8]));
            let t: RowTuple = (
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
            );
            let retries: i64 = r.get(12)?;
            Ok((row_to_stored(id, t), retries as u32))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn queue_depth(&self) -> Result<(u64, u64)> {
        let conn = self.conn.lock().unwrap();
        let outbound: u64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE delivery IN ('queued','sent')",
            [],
            |r| r.get(0),
        )?;
        let hold: u64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE hold_until > 0 AND suppressed = 0",
            [],
            |r| r.get(0),
        )?;
        Ok((outbound, hold))
    }

    pub fn heard_touch(
        &self,
        callsign: &str,
        snr: Option<f32>,
        mode: Option<&str>,
        gateway: bool,
        medium: &str,
        freq_khz: Option<u32>,
    ) -> Result<()> {
        let khz = freq_khz.filter(|k| *k > 0);
        let band = khz.and_then(crate::band::band_for_khz);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO heard(callsign, last_heard, snr, mode, gateway, medium, freq_khz, band)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(callsign) DO UPDATE SET
                last_heard = excluded.last_heard,
                snr = COALESCE(excluded.snr, heard.snr),
                mode = COALESCE(excluded.mode, heard.mode),
                gateway = excluded.gateway,
                medium = excluded.medium,
                freq_khz = CASE WHEN excluded.freq_khz > 0 THEN excluded.freq_khz ELSE heard.freq_khz END,
                band = CASE WHEN excluded.freq_khz > 0 THEN excluded.band ELSE heard.band END",
            params![
                callsign,
                Self::now() as i64,
                snr,
                mode,
                gateway as i64,
                medium,
                khz.unwrap_or(0) as i64,
                band,
            ],
        )?;
        Ok(())
    }

    pub fn heard_list(&self) -> Result<Vec<HeardStation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT callsign, last_heard, snr, mode, grid, gateway, medium, freq_khz, band FROM heard ORDER BY last_heard DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(HeardStation {
                callsign: r.get(0)?,
                last_heard: r.get::<_, i64>(1)? as u32,
                snr: r.get(2)?,
                mode: r.get(3)?,
                grid: r.get(4)?,
                gateway: r.get::<_, i64>(5)? != 0,
                medium: r.get(6)?,
                freq_khz: r.get::<_, i64>(7).unwrap_or(0) as u32,
                band: r.get(8)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn channels_for(&self, callsign: &str, since: u32) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT dest FROM messages
             WHERE origin = ?1 AND is_group = 1 AND rx_time >= ?2",
        )?;
        let rows = stmt.query_map(params![callsign, since as i64], |r| r.get::<_, String>(0))?;
        Ok(rows
            .filter_map(|r| r.ok())
            .filter(|d| !d.is_empty())
            .map(|d| {
                if d.starts_with('#') || d.starts_with('&') {
                    d
                } else {
                    format!("#{d}")
                }
            })
            .collect())
    }

    pub fn recently_heard(&self, callsign: &str, within_secs: u32) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let last: Option<i64> = conn
            .query_row(
                "SELECT last_heard FROM heard WHERE callsign = ?1",
                params![callsign],
                |r| r.get(0),
            )
            .optional()?;
        Ok(last
            .map(|t| Self::now() - t as u32 <= within_secs)
            .unwrap_or(false))
    }

    /// Heard on RF on this dial recently. Rows with `freq_khz == 0` never match.
    pub fn recently_heard_rf(
        &self,
        callsign: &str,
        within_secs: u32,
        freq_khz: u32,
    ) -> Result<bool> {
        if freq_khz == 0 {
            return Ok(false);
        }
        let conn = self.conn.lock().unwrap();
        let row: Option<(i64, String, i64)> = conn
            .query_row(
                "SELECT last_heard, medium, freq_khz FROM heard WHERE callsign = ?1",
                params![callsign],
                |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2).unwrap_or(0))),
            )
            .optional()?;
        Ok(row
            .map(|(last, medium, khz)| {
                medium == "rf"
                    && khz > 0
                    && khz as u32 == freq_khz
                    && Self::now().saturating_sub(last as u32) <= within_secs
            })
            .unwrap_or(false))
    }

    /// Any station heard on RF on this dial recently.
    pub fn recently_heard_any_rf(&self, within_secs: u32, freq_khz: u32) -> Result<bool> {
        if freq_khz == 0 {
            return Ok(false);
        }
        let cutoff = Self::now().saturating_sub(within_secs) as i64;
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM heard
             WHERE medium = 'rf' AND freq_khz = ?1 AND last_heard >= ?2",
            params![freq_khz as i64, cutoff],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn add_hop(&self, id: &MsgId, hop: &str, medium: &str, snr: Option<f32>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO hops(msg_id, hop_call, medium, ts, snr) VALUES(?1,?2,?3,?4,?5)",
            params![id.hex(), hop, medium, Self::now() as i64, snr],
        )?;
        Ok(())
    }

    pub fn rx_medium(&self, id: &MsgId) -> Result<Option<String>> {
        Ok(self
            .trace(id)?
            .into_iter()
            .next()
            .map(|(_, medium, _, _)| medium))
    }

    pub fn trace(&self, id: &MsgId) -> Result<Vec<(String, String, u32, Option<f32>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT hop_call, medium, ts, snr FROM hops WHERE msg_id = ?1 ORDER BY ts ASC",
        )?;
        let rows = stmt.query_map(params![id.hex()], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u32, r.get(3)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn group_create(&self, name: &str, members: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO groups(name, closed, created, default_prio) VALUES(?1, 1, ?2, 0)",
            params![name, Self::now() as i64],
        )?;
        for m in members {
            conn.execute(
                "INSERT OR IGNORE INTO group_members(group_name, callsign) VALUES(?1,?2)",
                params![name, m],
            )?;
        }
        Ok(())
    }

    pub fn group_members(&self, name: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT callsign FROM group_members WHERE group_name = ?1")?;
        let rows = stmt.query_map(params![name], |r| r.get(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn group_list(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name FROM groups ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Ensure a group row exists so channel defaults can be stored.
    pub fn group_ensure(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO groups(name, closed, created, default_prio) VALUES(?1, 1, ?2, 0)",
            params![name, Self::now() as i64],
        )?;
        Ok(())
    }

    pub fn group_set_prio(&self, name: &str, prio: u8) -> Result<()> {
        self.group_ensure(name)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE groups SET default_prio = ?1 WHERE name = ?2",
            params![prio as i64, name],
        )?;
        Ok(())
    }

    pub fn group_prio(&self, name: &str) -> Result<u8> {
        let conn = self.conn.lock().unwrap();
        let p: Option<i64> = conn
            .query_row(
                "SELECT default_prio FROM groups WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()?;
        Ok(p.unwrap_or(0).clamp(0, 2) as u8)
    }

    /// All stored channel default priorities as `(group_name, prio_bits)`.
    pub fn group_prios(&self) -> Result<Vec<(String, u8)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name, default_prio FROM groups ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u8))
        })?;
        Ok(rows
            .filter_map(|r| r.ok())
            .map(|(n, p)| (n, p.clamp(0, 2)))
            .collect())
    }

    pub fn receipt(&self, group: &str, msg_id: &MsgId, callsign: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO group_receipts(group_name, msg_id, callsign, seen_at)
             VALUES(?1,?2,?3,?4)",
            params![group, msg_id.hex(), callsign, Self::now() as i64],
        )?;
        Ok(())
    }

    pub fn receipts_for(&self, group: &str, msg_id: &MsgId) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT callsign FROM group_receipts WHERE group_name = ?1 AND msg_id = ?2")?;
        let rows = stmt.query_map(params![group, msg_id.hex()], |r| r.get(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn group_all_received(&self, group: &str, msg_id: &MsgId) -> Result<bool> {
        let members = self.group_members(group)?;
        if members.is_empty() {
            return Ok(false);
        }
        let got = self.receipts_for(group, msg_id)?;
        Ok(members.iter().all(|m| got.iter().any(|g| g == m)))
    }

    pub fn have_digest(
        &self,
        dest: &str,
        max_msgs: usize,
        max_age_hours: u64,
    ) -> Result<Vec<String>> {
        let cutoff = Self::now().saturating_sub((max_age_hours * 3600) as u32);
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT msg_id FROM messages WHERE dest = ?1 AND rx_time >= ?2
             ORDER BY rx_time DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![dest, cutoff as i64, max_msgs as i64], |r| r.get(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn checkin(
        &self,
        callsign: &str,
        note: &str,
        grid: Option<&str>,
        snr: Option<f32>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO checkins(callsign, note, at, grid, snr) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(callsign) DO UPDATE SET note=excluded.note, at=excluded.at, grid=excluded.grid, snr=excluded.snr",
            params![callsign, note, Self::now() as i64, grid, snr],
        )?;
        Ok(())
    }

    pub fn checkins(&self) -> Result<Vec<(String, String, u32, Option<String>, Option<f32>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT callsign, note, at, grid, snr FROM checkins ORDER BY at DESC")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get::<_, i64>(2)? as u32,
                r.get(3)?,
                r.get(4)?,
            ))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn welfare(&self, callsign: &str, code: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO welfare(callsign, code, at) VALUES(?1,?2,?3)
             ON CONFLICT(callsign) DO UPDATE SET code=excluded.code, at=excluded.at",
            params![callsign, code, Self::now() as i64],
        )?;
        Ok(())
    }

    pub fn welfare_of(&self, callsign: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT code FROM welfare WHERE callsign = ?1",
                params![callsign],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn set_mute(&self, target: &str, mute: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        if mute {
            conn.execute(
                "INSERT OR IGNORE INTO mutes(target) VALUES(?1)",
                params![target],
            )?;
        } else {
            conn.execute("DELETE FROM mutes WHERE target = ?1", params![target])?;
        }
        Ok(())
    }

    pub fn is_muted(&self, target: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mutes WHERE target = ?1",
            params![target],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn gc(&self) -> Result<()> {
        let cutoff = Self::now().saturating_sub((self.max_age_hours * 3600) as u32);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM seen WHERE first_seen < ?1",
            params![cutoff as i64],
        )?;
        // Keep conversation history forever; only bound the relay/hold window via hold_until.
        conn.execute(
            "UPDATE messages SET hold_until = 0 WHERE rx_time < ?1",
            params![cutoff as i64],
        )?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
        if count as u64 > self.max_msgs {
            let extra = count as u64 - self.max_msgs;
            conn.execute(
                "DELETE FROM messages WHERE msg_id IN (
                    SELECT msg_id FROM messages ORDER BY rx_time ASC LIMIT ?1
                )",
                params![extra as i64],
            )?;
        }
        Ok(())
    }
}

type RowTuple = (
    String,
    String,
    String,
    i64,
    i64,
    i64,
    i64,
    Vec<u8>,
    Option<Vec<u8>>,
    i64,
    String,
);

fn row_to_stored(id: MsgId, t: RowTuple) -> StoredMsg {
    let (kind, origin, dest, flags, hops, ts, seq, body, sig, rx, delivery) = t;
    StoredMsg {
        env: tuple_to_env(id, kind, origin, dest, flags, hops, ts, seq, body, sig),
        rx_time: rx as u32,
        delivery: Delivery::parse(&delivery),
    }
}

fn map_full_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMsg> {
    let hex: String = r.get(0)?;
    let id = MsgId::parse_hex(&hex).unwrap_or(MsgId([0; 8]));
    let t: RowTuple = (
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
        r.get(10)?,
        r.get(11)?,
    );
    Ok(row_to_stored(id, t))
}

fn tuple_to_env(
    id: MsgId,
    kind: String,
    origin: String,
    dest: String,
    flags: i64,
    hops: i64,
    ts: i64,
    seq: i64,
    body: Vec<u8>,
    sig: Option<Vec<u8>>,
) -> Envelope {
    let kind = match kind.as_str() {
        "ack" => MsgType::Ack,
        "beacon" => MsgType::Beacon,
        "have" => MsgType::Have,
        "want" => MsgType::Want,
        "ping" => MsgType::Ping,
        "checkin" => MsgType::Checkin,
        "status" => MsgType::Status,
        "form" => MsgType::Form,
        "file" => MsgType::File,
        "frag" => MsgType::Frag,
        _ => MsgType::Msg,
    };
    let mut signature = None;
    if let Some(s) = sig {
        if s.len() == 64 {
            let mut a = [0u8; 64];
            a.copy_from_slice(&s);
            signature = Some(a);
        }
    }
    Envelope {
        ver: crate::proto::VERSION,
        kind,
        flags: Flags(flags as u16),
        msg_id: id,
        origin: Callsign::from_raw(origin),
        dest: Callsign::from_raw(dest),
        hops_left: hops as u8,
        ts: ts as u32,
        seq: seq as u32,
        body,
        signature,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Envelope, FLAG_GROUP};

    #[test]
    fn bracket_marks() {
        assert_eq!(Delivery::Queued.ticks_bracket(), "[..]");
        assert_eq!(Delivery::Sent.ticks_bracket(), "[tx]");
        assert_eq!(Delivery::Relayed.ticks_bracket(), "[rl]");
        assert_eq!(Delivery::Delivered.ticks_bracket(), "[ok]");
        assert_eq!(Delivery::All.ticks_bracket(), "[all]");
        assert_eq!(via_bracket("rf"), "[rf]");
        assert_eq!(via_bracket("inet"), "[net]");
        assert_eq!(via_bracket("lan"), "[lan]");
        assert_eq!(via_bracket(""), "");
        assert_eq!(normalize_via(Some("peer")), Some("inet"));
        assert_eq!(normalize_via(None), None);
        assert_eq!(tries_bracket(0), "");
        assert_eq!(tries_bracket(1), "[x1]");
        assert_eq!(tries_bracket(3), "[x3]");
        let stamp = chat_stamp(Some("2026-09-18T12:00:00Z"));
        assert_eq!(stamp.len(), 19);
        assert!(stamp.chars().nth(10) == Some(' '));
        assert!(stamp.chars().nth(13) == Some(':'));
        assert!(stamp.chars().nth(16) == Some(':'));
    }

    #[test]
    fn rf_tx_counts_radio_sends() {
        let s = Store::open_memory().unwrap();
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"hi".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        s.insert(&env, Delivery::Queued).unwrap();
        assert_eq!(s.rf_tx_of(&env.msg_id).unwrap(), 0);
        assert_eq!(s.bump_rf_tx(&env.msg_id).unwrap(), 1);
        assert_eq!(s.bump_rf_tx(&env.msg_id).unwrap(), 2);
        assert_eq!(s.rf_tx_of(&env.msg_id).unwrap(), 2);
    }

    #[test]
    fn rx_medium_is_first_hop() {
        let s = Store::open_memory().unwrap();
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"hi".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        assert!(s.rx_medium(&env.msg_id).unwrap().is_none());
        s.add_hop(&env.msg_id, "G4ABC", "rf", Some(8.0)).unwrap();
        s.add_hop(&env.msg_id, "M0XYZ", "inet", None).unwrap();
        assert_eq!(s.rx_medium(&env.msg_id).unwrap().as_deref(), Some("rf"));
    }

    #[test]
    fn purge_channel_is_local_and_scoped() {
        let s = Store::open_memory().unwrap();
        let mut bulletin = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::from_raw("BULLETIN"),
            1,
            b"pub".to_vec(),
            3,
            Flags::new().with(FLAG_GROUP),
        )
        .unwrap();
        bulletin.flags.set(FLAG_GROUP, true);
        let mut ops = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::from_raw("OPS"),
            2,
            b"priv".to_vec(),
            3,
            Flags::new().with(FLAG_GROUP),
        )
        .unwrap();
        ops.flags.set(FLAG_GROUP, true);
        let dm = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            3,
            b"dm".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        s.insert(&bulletin, Delivery::Sent).unwrap();
        s.insert(&ops, Delivery::Sent).unwrap();
        s.insert(&dm, Delivery::Sent).unwrap();
        assert_eq!(s.purge_channel("#bulletin", Some("G4ABC")).unwrap(), 1);
        assert!(s.get(&bulletin.msg_id).unwrap().is_none());
        assert!(s.get(&ops.msg_id).unwrap().is_some());
        assert!(s.get(&dm.msg_id).unwrap().is_some());
        assert_eq!(s.purge_channel("M0XYZ", Some("G4ABC")).unwrap(), 1);
        assert!(s.get(&dm.msg_id).unwrap().is_none());
        assert!(s.get(&ops.msg_id).unwrap().is_some());
    }

    #[test]
    fn insert_and_dedupe() {
        let s = Store::open_memory().unwrap();
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"hi".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        assert!(s.insert(&env, Delivery::Queued).unwrap());
        assert!(!s.insert(&env, Delivery::Queued).unwrap());
        assert!(s.seen_before(&env.msg_id).unwrap());
        s.set_delivery(&env.msg_id, Delivery::Delivered).unwrap();
        assert_eq!(
            s.delivery_of(&env.msg_id).unwrap(),
            Some(Delivery::Delivered)
        );
    }

    #[test]
    fn retry_due_lists_unacked_sent() {
        let s = Store::open_memory().unwrap();
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"hi".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        s.insert(&env, Delivery::Sent).unwrap();
        s.set_hold(&env.msg_id, 1, 3).unwrap();
        let due = s.retry_due(10, "G4ABC", 3).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].1, 0);
        s.bump_retry(&env.msg_id, 20).unwrap();
        assert_eq!(s.retries_of(&env.msg_id).unwrap(), 1);
        assert!(s.retry_due(10, "G4ABC", 3).unwrap().is_empty());
        assert_eq!(s.retry_due(20, "G4ABC", 3).unwrap().len(), 1);
    }

    #[test]
    fn group_receipts() {
        let s = Store::open_memory().unwrap();
        s.group_create("net", &["G4ABC".into(), "M0XYZ".into()])
            .unwrap();
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::from_raw("NET"),
            1,
            b"hello".to_vec(),
            3,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap();
        s.insert(&env, Delivery::Sent).unwrap();
        s.receipt("net", &env.msg_id, "G4ABC").unwrap();
        assert!(!s.group_all_received("net", &env.msg_id).unwrap());
        s.receipt("net", &env.msg_id, "M0XYZ").unwrap();
        assert!(s.group_all_received("net", &env.msg_id).unwrap());
    }

    #[test]
    fn open_file_sets_wal() {
        let dir = std::env::temp_dir().join("wcr-store-wal-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("hub.db");
        Store::open(&p, 1, 10).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recently_heard_rf_filters_medium_and_dial() {
        let s = Store::open_memory().unwrap();
        s.heard_touch("G4ABC", None, None, false, "inet", Some(144950))
            .unwrap();
        assert!(!s.recently_heard_rf("G4ABC", 600, 144950).unwrap());
        assert!(s.recently_heard("G4ABC", 600).unwrap());

        s.heard_touch("G4ABC", None, None, false, "rf", Some(7045))
            .unwrap();
        assert!(!s.recently_heard_rf("G4ABC", 600, 144950).unwrap());
        assert!(s.recently_heard_rf("G4ABC", 600, 7045).unwrap());
        assert!(!s.recently_heard_rf("G4ABC", 600, 0).unwrap());

        s.heard_touch("M0XYZ", None, None, false, "rf", None)
            .unwrap();
        assert!(!s.recently_heard_rf("M0XYZ", 600, 144950).unwrap());

        s.heard_touch("M7TJF", None, None, false, "rf", Some(144950))
            .unwrap();
        assert!(s.recently_heard_any_rf(600, 144950).unwrap());
        assert!(s.recently_heard_any_rf(600, 7045).unwrap());
        assert!(!s.recently_heard_any_rf(600, 146520).unwrap());
    }
}
