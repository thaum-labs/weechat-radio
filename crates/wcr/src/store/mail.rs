//! SPDX-License-Identifier: Apache-2.0
//! Local SQLite mail folders + copy-to settings.

use super::{Delivery, Store};
use crate::error::Result;
use crate::mail::{trim_body, MAIL_MAX_BYTES};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

const MAIL_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS mail_messages (
    id TEXT PRIMARY KEY,
    folder TEXT NOT NULL,
    from_addr TEXT NOT NULL,
    to_addr TEXT NOT NULL,
    subject TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    created INTEGER NOT NULL,
    read_flag INTEGER NOT NULL DEFAULT 0,
    delivery TEXT NOT NULL DEFAULT 'queued',
    remote_id TEXT,
    byte_len INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_mail_folder ON mail_messages(folder, created DESC);

CREATE TABLE IF NOT EXISTS mail_kv (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailRow {
    pub id: String,
    pub folder: String,
    pub from_addr: String,
    pub to_addr: String,
    pub subject: String,
    pub body: String,
    pub created: i64,
    pub read: bool,
    pub delivery: String,
    pub remote_id: Option<String>,
    pub byte_len: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MailSettings {
    pub copy_to: String,
    pub copy_confirmed: bool,
    pub copy_pending_to: String,
    pub copy_pending_code: String,
}

impl Store {
    pub fn ensure_mail_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(MAIL_SCHEMA)?;
        Ok(())
    }

    pub fn mail_insert(
        &self,
        id: &str,
        folder: &str,
        from_addr: &str,
        to_addr: &str,
        subject: &str,
        body: &str,
        delivery: Delivery,
        remote_id: Option<&str>,
    ) -> Result<()> {
        let body = trim_body(body)?;
        let now = chrono::Utc::now().timestamp();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO mail_messages(id, folder, from_addr, to_addr, subject, body, created, read_flag, delivery, remote_id, byte_len)
             VALUES(?1,?2,?3,?4,?5,?6,?7, COALESCE((SELECT read_flag FROM mail_messages WHERE id=?1),0), ?8, ?9, ?10)",
            params![
                id,
                folder,
                from_addr,
                to_addr,
                subject,
                body,
                now,
                delivery.as_str(),
                remote_id,
                body.len() as i64,
            ],
        )?;
        Ok(())
    }

    pub fn mail_list(&self, folder: &str, limit: i64) -> Result<Vec<MailRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, folder, from_addr, to_addr, subject, body, created, read_flag, delivery, remote_id, byte_len
             FROM mail_messages WHERE folder = ?1 ORDER BY created DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![folder, limit], |r| {
            Ok(MailRow {
                id: r.get(0)?,
                folder: r.get(1)?,
                from_addr: r.get(2)?,
                to_addr: r.get(3)?,
                subject: r.get(4)?,
                body: r.get(5)?,
                created: r.get(6)?,
                read: r.get::<_, i64>(7)? != 0,
                delivery: r.get(8)?,
                remote_id: r.get(9)?,
                byte_len: r.get::<_, i64>(10)? as usize,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn mail_get(&self, id: &str) -> Result<Option<MailRow>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, folder, from_addr, to_addr, subject, body, created, read_flag, delivery, remote_id, byte_len
             FROM mail_messages WHERE id = ?1",
            params![id],
            |r| {
                Ok(MailRow {
                    id: r.get(0)?,
                    folder: r.get(1)?,
                    from_addr: r.get(2)?,
                    to_addr: r.get(3)?,
                    subject: r.get(4)?,
                    body: r.get(5)?,
                    created: r.get(6)?,
                    read: r.get::<_, i64>(7)? != 0,
                    delivery: r.get(8)?,
                    remote_id: r.get(9)?,
                    byte_len: r.get::<_, i64>(10)? as usize,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn mail_set_read(&self, id: &str, read: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE mail_messages SET read_flag = ?2 WHERE id = ?1",
            params![id, read as i64],
        )?;
        Ok(())
    }

    pub fn mail_set_delivery(&self, id: &str, delivery: Delivery) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE mail_messages SET delivery = ?2 WHERE id = ?1",
            params![id, delivery.as_str()],
        )?;
        Ok(())
    }

    pub fn mail_move_folder(&self, id: &str, folder: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE mail_messages SET folder = ?2 WHERE id = ?1",
            params![id, folder],
        )?;
        Ok(())
    }

    pub fn mail_unread_count(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mail_messages WHERE folder = 'inbox' AND read_flag = 0",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    pub fn mail_settings(&self) -> Result<MailSettings> {
        let conn = self.conn.lock().unwrap();
        let mut s = MailSettings::default();
        let mut stmt = conn.prepare("SELECT key, value FROM mail_kv")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows.flatten() {
            match row.0.as_str() {
                "copy_to" => s.copy_to = row.1,
                "copy_confirmed" => s.copy_confirmed = row.1 == "1",
                "copy_pending_to" => s.copy_pending_to = row.1,
                "copy_pending_code" => s.copy_pending_code = row.1,
                _ => {}
            }
        }
        Ok(s)
    }

    pub fn mail_set_kv(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO mail_kv(key, value) VALUES(?1,?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn mail_cap_check(body: &str) -> Result<()> {
        if body.len() > MAIL_MAX_BYTES {
            return Err(crate::error::Error::config(format!(
                "body is {} bytes; max {}",
                body.len(),
                MAIL_MAX_BYTES
            )));
        }
        Ok(())
    }
}
