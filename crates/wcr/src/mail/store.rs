//! SPDX-License-Identifier: Apache-2.0
//! Email store. Separate from the chat `messages` table.

use crate::error::{Error, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS mail (
    id TEXT PRIMARY KEY,
    folder TEXT NOT NULL,
    from_addr TEXT NOT NULL,
    to_addr TEXT NOT NULL,
    subject TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    ts INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued',
    unread INTEGER NOT NULL DEFAULT 0,
    ack INTEGER NOT NULL DEFAULT 0,
    retries INTEGER NOT NULL DEFAULT 0,
    detail TEXT NOT NULL DEFAULT '',
    chunks INTEGER NOT NULL DEFAULT 0,
    got TEXT NOT NULL DEFAULT '',
    delivery TEXT NOT NULL DEFAULT '',
    delivery_ts INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS mail_hop (
    id TEXT NOT NULL,
    stage TEXT NOT NULL,
    ts INTEGER NOT NULL,
    PRIMARY KEY (id, stage)
);
CREATE TABLE IF NOT EXISTS mail_wait (
    id TEXT PRIMARY KEY,
    from_addr TEXT NOT NULL,
    subject TEXT NOT NULL DEFAULT '',
    bytes INTEGER NOT NULL DEFAULT 0,
    ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS mail_copy (
    callsign TEXT PRIMARY KEY,
    address TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 0,
    confirmed INTEGER NOT NULL DEFAULT 0,
    pending TEXT NOT NULL DEFAULT '',
    code TEXT NOT NULL DEFAULT '',
    hub_wait INTEGER NOT NULL DEFAULT 0
);
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MailRow {
    pub id: String,
    pub folder: String,
    pub from_addr: String,
    pub to_addr: String,
    pub subject: String,
    pub body: String,
    pub ts: i64,
    pub state: String,
    pub unread: bool,
    pub ack: bool,
    pub retries: u32,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub chunks: u32,
    #[serde(default)]
    pub got: String,
    /// Resend's last word on a sent mail, from the hub: `delivered`,
    /// `bounced`, `delivery_delayed`, … Empty until the first check.
    #[serde(default)]
    pub delivery: String,
}

/// When a mail reached one stage: `air`, `gateway`, `sent` (Resend
/// accepted it), `failed`, `report` (Resend's final answer) or `fetched`
/// (an inbound mail arrived here). The first time counts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hop {
    pub stage: String,
    pub ts: i64,
}

/// The hop a state change reaches, if any.
fn hop_for_state(state: &str) -> Option<&'static str> {
    match state {
        "tx" => Some("air"),
        "gateway" => Some("gateway"),
        "sent" => Some("sent"),
        "failed" => Some("failed"),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WaitHeader {
    pub id: String,
    pub from_addr: String,
    pub subject: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CopySettings {
    pub address: String,
    pub enabled: bool,
    pub confirmed: bool,
    pub pending: String,
    /// `off`, `code sent`, `hub not confirmed`, or `on`.
    #[serde(default)]
    pub state: String,
}

pub const COPY_OFF: &str = "off";
pub const COPY_CODE_SENT: &str = "code sent";
/// The code matched here but the hub has not taken it yet. The code is kept.
pub const COPY_HUB_WAIT: &str = "hub not confirmed";
pub const COPY_ON: &str = "on";

const MAIL_COLS: &str =
    "id, folder, from_addr, to_addr, subject, body, ts, state, unread, ack, retries, detail, chunks, got, delivery";

/// Where a Radio+ Get stands, for the Email tab's Get window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GetProgress {
    /// "", "asking", "receiving", "fetched", or "failed".
    pub state: String,
    pub subject: String,
    /// The waiting mail id the Get asked for, so the log names one id.
    #[serde(default)]
    pub asked: String,
    pub got: u16,
    pub total: u16,
    /// Why it failed.
    pub detail: String,
}

impl GetProgress {
    pub fn active(&self) -> bool {
        matches!(self.state.as_str(), "asking" | "receiving")
    }
}

pub struct MailStore {
    conn: Mutex<Connection>,
    /// Did the last delivery check reach the hub? `None` before the first.
    hub_reach: Mutex<Option<bool>>,
    /// When another station's mail exchange was last heard here.
    other_mail: Mutex<Option<std::time::Instant>>,
    notice: Mutex<String>,
    get: Mutex<GetProgress>,
}

impl MailStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            hub_reach: Mutex::new(None),
            other_mail: Mutex::new(None),
            notice: Mutex::new(String::new()),
            get: Mutex::new(GetProgress::default()),
        })
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            hub_reach: Mutex::new(None),
            other_mail: Mutex::new(None),
            notice: Mutex::new(String::new()),
            get: Mutex::new(GetProgress::default()),
        })
    }

    pub fn set_notice(&self, text: &str) {
        *self.notice.lock() = text.to_string();
    }

    pub fn notice(&self) -> String {
        self.notice.lock().clone()
    }

    pub fn get_progress(&self) -> GetProgress {
        self.get.lock().clone()
    }

    pub fn set_get_progress(&self, p: GetProgress) {
        *self.get.lock() = p;
    }

    /// Chunks of the fetched mail so far. The count never goes back.
    pub fn note_get_chunks(&self, got: u16, total: u16) -> bool {
        let mut g = self.get.lock();
        if !g.active() || total == 0 {
            return false;
        }
        let got = got.min(total).max(if g.total == total { g.got } else { 0 });
        let moved = g.state != "receiving" || g.got != got || g.total != total;
        g.state = "receiving".into();
        g.got = got;
        g.total = total;
        moved
    }

    pub fn insert(&self, row: &MailRow) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO mail(id, folder, from_addr, to_addr, subject, body, ts, state, unread, ack, retries, detail, chunks, got, delivery)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                row.id,
                row.folder,
                row.from_addr,
                row.to_addr,
                row.subject,
                row.body,
                row.ts,
                row.state,
                row.unread as i64,
                row.ack as i64,
                row.retries,
                row.detail,
                row.chunks,
                row.got,
                row.delivery,
            ],
        )?;
        // A new row (or the same mail composed again) starts a new timeline.
        conn.execute("DELETE FROM mail_hop WHERE id = ?1", params![row.id])?;
        if row.folder == "inbox" {
            add_hop(&conn, &row.id, "fetched", chrono::Utc::now().timestamp())?;
        }
        Ok(())
    }

    /// Times this mail reached each stage, in the order it reached them.
    pub fn hops(&self, id: &str) -> Result<Vec<Hop>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT stage, ts FROM mail_hop WHERE id = ?1 ORDER BY rowid ASC")?;
        let rows = stmt.query_map(params![id], |r| {
            Ok(Hop {
                stage: r.get(0)?,
                ts: r.get(1)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    pub fn note_hub_reach(&self, reached: bool) {
        *self.hub_reach.lock() = Some(reached);
    }

    /// False only when the last delivery check could not reach the hub.
    pub fn hub_reachable(&self) -> bool {
        self.hub_reach.lock().unwrap_or(true)
    }

    /// Another station's mail chunk, or the gateway's reply to it, was just heard.
    pub fn note_other_mail(&self) {
        *self.other_mail.lock() = Some(std::time::Instant::now());
    }

    /// Time since another station's mail exchange was heard, if ever.
    pub fn since_other_mail(&self) -> Option<std::time::Duration> {
        self.other_mail.lock().map(|t| t.elapsed())
    }

    /// Manual Retry of a Failed mail: back to the outbox as a fresh send.
    /// Returns false unless the mail is Failed.
    pub fn reset_for_retry(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let n = conn.execute(
            "UPDATE mail SET folder = 'outbox', state = 'queued', retries = 0, detail = '' WHERE id = ?1 AND state = 'failed'",
            params![id],
        )?;
        if n == 1 {
            conn.execute("DELETE FROM mail_hop WHERE id = ?1", params![id])?;
        }
        Ok(n == 1)
    }

    pub fn get(&self, id: &str) -> Result<Option<MailRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!("SELECT {MAIL_COLS} FROM mail WHERE id = ?1"))?;
        let mut rows = stmt.query(params![id])?;
        if let Some(r) = rows.next()? {
            Ok(Some(map_row(r)?))
        } else {
            Ok(None)
        }
    }

    pub fn list(&self, folder: &str) -> Result<Vec<MailRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {MAIL_COLS} FROM mail WHERE folder = ?1 ORDER BY ts DESC"
        ))?;
        let rows = stmt.query_map(params![folder], map_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    pub fn set_state(&self, id: &str, folder: &str, state: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE mail SET folder = ?2, state = ?3, unread = CASE WHEN ?2 = 'inbox' THEN 1 ELSE unread END WHERE id = ?1",
            params![id, folder, state],
        )?;
        if let Some(stage) = hop_for_state(state) {
            add_hop(&conn, id, stage, chrono::Utc::now().timestamp())?;
        }
        Ok(())
    }

    pub fn mark_read(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .execute("UPDATE mail SET unread = 0 WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn set_outcome(&self, id: &str, folder: &str, state: &str, detail: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE mail SET folder = ?2, state = ?3, detail = ?4, unread = CASE WHEN ?2 = 'inbox' THEN 1 ELSE unread END WHERE id = ?1",
            params![id, folder, state, detail],
        )?;
        if let Some(stage) = hop_for_state(state) {
            add_hop(&conn, id, stage, chrono::Utc::now().timestamp())?;
        }
        Ok(())
    }

    pub fn set_chunks(&self, id: &str, chunks: u32) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE mail SET chunks = ?2 WHERE id = ?1",
            params![id, chunks],
        )?;
        Ok(())
    }

    /// Record a gateway chunk ack. Returns `(have, total)` when the row exists.
    pub fn note_chunk(&self, id: &str, idx: u16) -> Result<Option<(u32, u32)>> {
        let Some(row) = self.get(id)? else {
            return Ok(None);
        };
        let mut have: Vec<u16> = row
            .got
            .split(',')
            .filter_map(|s| s.parse::<u16>().ok())
            .collect();
        if !have.contains(&idx) {
            have.push(idx);
            have.sort_unstable();
        }
        let got = have
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");
        self.conn
            .lock()
            .execute("UPDATE mail SET got = ?2 WHERE id = ?1", params![id, got])?;
        Ok(Some((have.len() as u32, row.chunks)))
    }

    /// Save the hub's delivery answer and when it was asked.
    pub fn set_delivery(&self, id: &str, delivery: &str, now: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE mail SET delivery = ?2, delivery_ts = ?3 WHERE id = ?1",
            params![id, delivery, now],
        )?;
        if super::progress::delivery_final(delivery) {
            add_hop(&conn, id, "report", now)?;
        }
        Ok(())
    }

    /// Sent mail whose delivery check is due at `now`. See
    /// [`super::progress::delivery_check_due`].
    pub fn delivery_due(&self, now: i64) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, ts, delivery, delivery_ts FROM mail WHERE folder = 'sent' AND state = 'sent'",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let due = rows
            .filter_map(|r| r.ok())
            .filter(|(_, ts, delivery, checked)| {
                super::progress::delivery_check_due(now - ts, now - checked, delivery)
            })
            .map(|(id, ..)| id)
            .collect();
        Ok(due)
    }

    pub fn bump_retry(&self, id: &str) -> Result<u32> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE mail SET retries = retries + 1, state = 'queued', folder = 'outbox' WHERE id = ?1",
            params![id],
        )?;
        let n: i64 =
            conn.query_row("SELECT retries FROM mail WHERE id = ?1", params![id], |r| {
                r.get(0)
            })?;
        Ok(n as u32)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let n = conn.execute("DELETE FROM mail WHERE id = ?1", params![id])?;
        conn.execute("DELETE FROM mail_hop WHERE id = ?1", params![id])?;
        Ok(n == 1)
    }

    pub fn unread(&self) -> Result<u64> {
        let n: i64 = self.conn.lock().query_row(
            "SELECT COUNT(*) FROM mail WHERE folder = 'inbox' AND unread = 1",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// Hide one waiting header here. The hub keeps the mail until it is
    /// fetched, so the next check lists it again.
    pub fn delete_waiting(&self, id: &str) -> Result<bool> {
        let n = self
            .conn
            .lock()
            .execute("DELETE FROM mail_wait WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    /// The waiting header for a mail that has just arrived. The wire does not
    /// carry the hub's id, so it is matched by sender and subject.
    pub fn clear_waiting_for(&self, from_addr: &str, subject: &str) -> Result<()> {
        self.conn.lock().execute(
            "DELETE FROM mail_wait WHERE id IN (SELECT id FROM mail_wait WHERE from_addr = ?1 AND subject = ?2 ORDER BY ts ASC LIMIT 1)",
            params![from_addr, subject],
        )?;
        Ok(())
    }

    pub fn replace_waiting(&self, rows: &[WaitHeader]) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM mail_wait", [])?;
        let ts = chrono::Utc::now().timestamp();
        for row in rows {
            conn.execute(
                "INSERT INTO mail_wait(id, from_addr, subject, bytes, ts) VALUES(?1,?2,?3,?4,?5)",
                params![row.id, row.from_addr, row.subject, row.bytes as i64, ts],
            )?;
        }
        Ok(())
    }

    pub fn waiting(&self) -> Result<Vec<WaitHeader>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT id, from_addr, subject, bytes FROM mail_wait ORDER BY ts ASC")?;
        let rows = stmt.query_map([], |r| {
            Ok(WaitHeader {
                id: r.get(0)?,
                from_addr: r.get(1)?,
                subject: r.get(2)?,
                bytes: r.get::<_, i64>(3)? as usize,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    pub fn copy(&self, callsign: &str) -> Result<CopySettings> {
        let conn = self.conn.lock();
        let row = conn.query_row(
            "SELECT address, enabled, confirmed, pending, hub_wait FROM mail_copy WHERE callsign = ?1",
            params![callsign.to_ascii_uppercase()],
            |r| {
                let mut s = CopySettings {
                    address: r.get(0)?,
                    enabled: r.get::<_, i64>(1)? != 0,
                    confirmed: r.get::<_, i64>(2)? != 0,
                    pending: r.get(3)?,
                    state: String::new(),
                };
                let hub_wait = r.get::<_, i64>(4)? != 0;
                s.state = if !s.pending.is_empty() && hub_wait {
                    COPY_HUB_WAIT
                } else if !s.pending.is_empty() {
                    COPY_CODE_SENT
                } else if s.enabled && s.confirmed {
                    COPY_ON
                } else {
                    COPY_OFF
                }
                .to_string();
                Ok(s)
            },
        );
        match row {
            Ok(s) => Ok(s),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(CopySettings {
                state: COPY_OFF.into(),
                ..CopySettings::default()
            }),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_copy_pending(&self, callsign: &str, address: &str, code: &str) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO mail_copy(callsign, address, enabled, confirmed, pending, code, hub_wait)
             VALUES(?1, COALESCE((SELECT address FROM mail_copy WHERE callsign=?1), ''), 0, 0, ?2, ?3, 0)
             ON CONFLICT(callsign) DO UPDATE SET pending = excluded.pending, code = excluded.code, confirmed = 0, hub_wait = 0",
            params![callsign.to_ascii_uppercase(), address, code],
        )?;
        Ok(())
    }

    /// Does `code` match the one emailed? Nothing is consumed.
    pub fn copy_code_matches(&self, callsign: &str, code: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let row: Option<(String, String)> = match conn.query_row(
            "SELECT pending, code FROM mail_copy WHERE callsign = ?1",
            params![callsign.to_ascii_uppercase()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.into()),
        };
        Ok(row.is_some_and(|(pending, expect)| {
            !expect.is_empty() && !pending.is_empty() && expect == code
        }))
    }

    /// The code matched but the hub did not take it. Keep the code for a retry.
    pub fn mark_copy_hub_wait(&self, callsign: &str) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE mail_copy SET hub_wait = 1 WHERE callsign = ?1",
            params![callsign.to_ascii_uppercase()],
        )?;
        Ok(())
    }

    /// Consumes the code. Call only once the hub has confirmed it too.
    pub fn confirm_copy(&self, callsign: &str, code: &str) -> Result<bool> {
        if !self.copy_code_matches(callsign, code)? {
            return Ok(false);
        }
        self.conn.lock().execute(
            "UPDATE mail_copy SET address = pending, enabled = 1, confirmed = 1, pending = '', code = '', hub_wait = 0 WHERE callsign = ?1",
            params![callsign.to_ascii_uppercase()],
        )?;
        Ok(true)
    }

    /// Copy-to off: no address, no pending code.
    pub fn clear_copy(&self, callsign: &str) -> Result<()> {
        self.conn.lock().execute(
            "DELETE FROM mail_copy WHERE callsign = ?1",
            params![callsign.to_ascii_uppercase()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_progress_counts_chunks_and_never_goes_back() {
        let store = MailStore::open_memory().unwrap();
        // No Get under way: chunks are not a Get's.
        assert!(!store.note_get_chunks(1, 6));
        store.set_get_progress(GetProgress {
            state: "asking".into(),
            ..GetProgress::default()
        });
        assert!(store.note_get_chunks(2, 6));
        assert!(!store.note_get_chunks(1, 6));
        let p = store.get_progress();
        assert_eq!((p.state.as_str(), p.got, p.total), ("receiving", 2, 6));
        assert!(store.note_get_chunks(9, 6));
        assert_eq!(store.get_progress().got, 6);
    }

    fn out_row(id: &str) -> MailRow {
        MailRow {
            id: id.into(),
            folder: "outbox".into(),
            state: "queued".into(),
            ts: 100,
            ack: true,
            ..MailRow::default()
        }
    }

    #[test]
    fn each_stage_keeps_its_first_time_and_a_report_ends_the_timeline() {
        let store = MailStore::open_memory().unwrap();
        store.insert(&out_row("m1")).unwrap();
        assert!(store.hops("m1").unwrap().is_empty());
        store.set_state("m1", "outbox", "tx").unwrap();
        store.set_state("m1", "outbox", "tx").unwrap();
        store.set_outcome("m1", "outbox", "gateway", "").unwrap();
        store.set_outcome("m1", "sent", "sent", "").unwrap();
        store.set_delivery("m1", "sent", 500).unwrap();
        store.set_delivery("m1", "delivered", 600).unwrap();
        store.set_delivery("m1", "delivered", 700).unwrap();
        let hops = store.hops("m1").unwrap();
        let stages: Vec<&str> = hops.iter().map(|h| h.stage.as_str()).collect();
        assert_eq!(stages, ["air", "gateway", "sent", "report"]);
        assert_eq!(hops[3].ts, 600);
        assert!(store.delete("m1").unwrap());
        assert!(store.hops("m1").unwrap().is_empty());
    }

    #[test]
    fn inbound_mail_records_when_it_was_fetched() {
        let store = MailStore::open_memory().unwrap();
        store
            .insert(&MailRow {
                folder: "inbox".into(),
                state: "received".into(),
                unread: true,
                ..out_row("in1")
            })
            .unwrap();
        let hops = store.hops("in1").unwrap();
        assert_eq!(hops.len(), 1);
        assert_eq!(hops[0].stage, "fetched");
    }

    #[test]
    fn retry_takes_only_failed_mail_back_to_the_outbox_as_a_fresh_send() {
        let store = MailStore::open_memory().unwrap();
        store.insert(&out_row("m1")).unwrap();
        store.set_state("m1", "outbox", "tx").unwrap();
        assert!(!store.reset_for_retry("m1").unwrap());
        store.bump_retry("m1").unwrap();
        store
            .set_outcome("m1", "outbox", "failed", "Gateway did not acknowledge.")
            .unwrap();
        assert!(store.reset_for_retry("m1").unwrap());
        let row = store.get("m1").unwrap().unwrap();
        assert_eq!(
            (
                row.folder.as_str(),
                row.state.as_str(),
                row.retries,
                row.detail.as_str()
            ),
            ("outbox", "queued", 0, "")
        );
        assert!(store.hops("m1").unwrap().is_empty());
    }

    #[test]
    fn a_fetched_mail_clears_its_waiting_header_and_a_header_can_be_deleted() {
        let store = MailStore::open_memory().unwrap();
        let w = |id: &str, subject: &str| WaitHeader {
            id: id.into(),
            from_addr: "a@example.com".into(),
            subject: subject.into(),
            bytes: 10,
        };
        store
            .replace_waiting(&[w("h1", "Re: Test"), w("h2", "Other")])
            .unwrap();
        store
            .clear_waiting_for("a@example.com", "Re: Test")
            .unwrap();
        let left = store.waiting().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, "h2");
        assert!(store.delete_waiting("h2").unwrap());
        assert!(store.waiting().unwrap().is_empty());
    }
}

/// The first time a mail reaches a stage is kept.
fn add_hop(conn: &Connection, id: &str, stage: &str, ts: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO mail_hop(id, stage, ts) VALUES(?1, ?2, ?3)",
        params![id, stage, ts],
    )?;
    Ok(())
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MailRow> {
    Ok(MailRow {
        id: r.get(0)?,
        folder: r.get(1)?,
        from_addr: r.get(2)?,
        to_addr: r.get(3)?,
        subject: r.get(4)?,
        body: r.get(5)?,
        ts: r.get(6)?,
        state: r.get(7)?,
        unread: r.get::<_, i64>(8)? != 0,
        ack: r.get::<_, i64>(9)? != 0,
        retries: r.get::<_, i64>(10)? as u32,
        detail: r.get(11)?,
        chunks: r.get::<_, i64>(12)? as u32,
        got: r.get(13)?,
        delivery: r.get(14)?,
    })
}

fn migrate(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(mail)")?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();
    if !names.iter().any(|n| n == "detail") {
        conn.execute(
            "ALTER TABLE mail ADD COLUMN detail TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    if !names.iter().any(|n| n == "chunks") {
        conn.execute(
            "ALTER TABLE mail ADD COLUMN chunks INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !names.iter().any(|n| n == "got") {
        conn.execute(
            "ALTER TABLE mail ADD COLUMN got TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    if !names.iter().any(|n| n == "delivery") {
        conn.execute(
            "ALTER TABLE mail ADD COLUMN delivery TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    if !names.iter().any(|n| n == "delivery_ts") {
        conn.execute(
            "ALTER TABLE mail ADD COLUMN delivery_ts INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    let mut stmt = conn.prepare("PRAGMA table_info(mail_copy)")?;
    let copy_cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();
    if !copy_cols.iter().any(|n| n == "hub_wait") {
        conn.execute(
            "ALTER TABLE mail_copy ADD COLUMN hub_wait INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}
