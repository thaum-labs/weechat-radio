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
    got TEXT NOT NULL DEFAULT ''
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
    code TEXT NOT NULL DEFAULT ''
);
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
}

const MAIL_COLS: &str =
    "id, folder, from_addr, to_addr, subject, body, ts, state, unread, ack, retries, detail, chunks, got";

pub struct MailStore {
    conn: Mutex<Connection>,
    notice: Mutex<String>,
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
            notice: Mutex::new(String::new()),
        })
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            notice: Mutex::new(String::new()),
        })
    }

    pub fn set_notice(&self, text: &str) {
        *self.notice.lock() = text.to_string();
    }

    pub fn notice(&self) -> String {
        self.notice.lock().clone()
    }

    pub fn insert(&self, row: &MailRow) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO mail(id, folder, from_addr, to_addr, subject, body, ts, state, unread, ack, retries, detail, chunks, got)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
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
            ],
        )?;
        Ok(())
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
        let mut have: Vec<u16> = row.got.split(',').filter_map(|s| s.parse().ok()).collect();
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
        let n = self
            .conn
            .lock()
            .execute("DELETE FROM mail WHERE id = ?1", params![id])?;
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
            "SELECT address, enabled, confirmed, pending FROM mail_copy WHERE callsign = ?1",
            params![callsign.to_ascii_uppercase()],
            |r| {
                Ok(CopySettings {
                    address: r.get(0)?,
                    enabled: r.get::<_, i64>(1)? != 0,
                    confirmed: r.get::<_, i64>(2)? != 0,
                    pending: r.get(3)?,
                })
            },
        );
        match row {
            Ok(s) => Ok(s),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(CopySettings::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_copy_pending(&self, callsign: &str, address: &str, code: &str) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO mail_copy(callsign, address, enabled, confirmed, pending, code)
             VALUES(?1, COALESCE((SELECT address FROM mail_copy WHERE callsign=?1), ''), 0, 0, ?2, ?3)
             ON CONFLICT(callsign) DO UPDATE SET pending = excluded.pending, code = excluded.code, confirmed = 0",
            params![callsign.to_ascii_uppercase(), address, code],
        )?;
        Ok(())
    }

    pub fn confirm_copy(&self, callsign: &str, code: &str) -> Result<bool> {
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
        if let Some((pending, expect)) = row {
            if !expect.is_empty() && expect == code && !pending.is_empty() {
                conn.execute(
                    "UPDATE mail_copy SET address = ?2, enabled = 1, confirmed = 1, pending = '', code = '' WHERE callsign = ?1",
                    params![callsign.to_ascii_uppercase(), pending],
                )?;
                return Ok(true);
            }
        }
        Ok(false)
    }
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
    Ok(())
}
