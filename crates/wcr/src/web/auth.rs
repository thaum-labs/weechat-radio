//! SPDX-License-Identifier: Apache-2.0
//! The web page's one password and its login sessions.

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A login lasts this long without being used.
pub const SESSION_IDLE: Duration = Duration::from_secs(12 * 60 * 60);

/// Shortest password `wcr web password` takes.
pub const MIN_PASSWORD: usize = 8;

pub const COOKIE: &str = "wcr_session";

pub fn hash_password(password: &str) -> Result<String, String> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(format!("Use at least {MIN_PASSWORD} characters."));
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

pub fn password_ok(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Logged-in browsers, by session token. Kept in memory: a restart logs
/// everyone out, which is fine for one station's page.
#[derive(Default)]
pub struct Sessions {
    live: Mutex<HashMap<String, Instant>>,
}

impl Sessions {
    pub fn open(&self) -> String {
        let token = hex::encode(rand::random::<[u8; 32]>());
        let mut live = self.live.lock();
        live.retain(|_, seen| seen.elapsed() < SESSION_IDLE);
        live.insert(token.clone(), Instant::now());
        token
    }

    /// True, and the session's clock restarts, when the token is live.
    pub fn check(&self, token: &str) -> bool {
        let mut live = self.live.lock();
        match live.get_mut(token) {
            Some(seen) if seen.elapsed() < SESSION_IDLE => {
                *seen = Instant::now();
                true
            }
            Some(_) => {
                live.remove(token);
                false
            }
            None => false,
        }
    }

    /// True while the token is live, without restarting its clock: an open
    /// page's event stream does not keep a login alive by itself.
    pub fn alive(&self, token: &str) -> bool {
        self.live
            .lock()
            .get(token)
            .is_some_and(|seen| seen.elapsed() < SESSION_IDLE)
    }

    pub fn close(&self, token: &str) {
        self.live.lock().remove(token);
    }

    /// Hand the live logins to the next `wcr web` across a restart after an
    /// update (#280), so the page reloads without asking for the password.
    /// The file is readable by this user only and read back once.
    pub fn save_for_restart(&self, path: &std::path::Path) -> std::io::Result<()> {
        let live: Vec<(String, u64)> = self
            .live
            .lock()
            .iter()
            .filter(|(_, seen)| seen.elapsed() < SESSION_IDLE)
            .map(|(t, seen)| (t.clone(), seen.elapsed().as_secs()))
            .collect();
        let text = serde_json::to_string(&live).map_err(std::io::Error::other)?;
        let _ = std::fs::remove_file(path);
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        use std::io::Write;
        opts.open(path)?.write_all(text.as_bytes())
    }

    /// Take back the logins `save_for_restart` left, if it was just now, and
    /// remove the file either way.
    pub fn restore_after_restart(&self, path: &std::path::Path) -> usize {
        let fresh = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age < RESTART_HANDOFF);
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let _ = std::fs::remove_file(path);
        if !fresh {
            return 0;
        }
        let Ok(list) = serde_json::from_str::<Vec<(String, u64)>>(&text) else {
            return 0;
        };
        let mut live = self.live.lock();
        let now = Instant::now();
        for (token, idle) in &list {
            let seen = now.checked_sub(Duration::from_secs(*idle)).unwrap_or(now);
            live.insert(token.clone(), seen);
        }
        list.len()
    }
}

/// Logins handed over by a restart are taken back only this soon after.
const RESTART_HANDOFF: Duration = Duration::from_secs(120);

/// The session token in a `Cookie` header, if any.
pub fn cookie_token(header: &str) -> Option<&str> {
    header
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value)
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_matches_only_its_own_hash() {
        let hash = hash_password("correct horse").unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(password_ok("correct horse", &hash));
        assert!(!password_ok("wrong horse", &hash));
        assert!(!password_ok("correct horse", ""));
        assert!(hash_password("short").is_err());
    }

    #[test]
    fn sessions_open_check_and_close() {
        let s = Sessions::default();
        let t = s.open();
        assert_eq!(t.len(), 64);
        assert!(s.check(&t));
        assert!(!s.check("nope"));
        s.close(&t);
        assert!(!s.check(&t));
    }

    #[test]
    fn logins_survive_a_restart_after_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("web-sessions");
        let s = Sessions::default();
        let t = s.open();
        s.save_for_restart(&file).unwrap();
        let next = Sessions::default();
        assert_eq!(next.restore_after_restart(&file), 1);
        assert!(next.check(&t));
        assert!(!file.exists(), "read once");
        assert_eq!(Sessions::default().restore_after_restart(&file), 0);
    }

    #[test]
    fn the_cookie_is_found_among_others() {
        assert_eq!(cookie_token("a=1; wcr_session=abc; b=2"), Some("abc"));
        assert_eq!(cookie_token("wcr_session="), None);
        assert_eq!(cookie_token("other=1"), None);
    }
}
