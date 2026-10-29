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

    pub fn close(&self, token: &str) {
        self.live.lock().remove(token);
    }
}

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
    fn the_cookie_is_found_among_others() {
        assert_eq!(cookie_token("a=1; wcr_session=abc; b=2"), Some("abc"));
        assert_eq!(cookie_token("wcr_session="), None);
        assert_eq!(cookie_token("other=1"), None);
    }
}
