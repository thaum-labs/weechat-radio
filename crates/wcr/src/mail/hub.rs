//! SPDX-License-Identifier: Apache-2.0
//! HTTP client for `/api/v1/mail/*`. Not the chat hub WebSocket.

use crate::error::{Error, Result};
use crate::proto::IdentityKeys;
use serde_json::{json, Value};

pub fn http_base_from_hub(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let mut s = if let Some(rest) = trimmed.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = trimmed.strip_prefix("ws://") {
        format!("http://{rest}")
    } else {
        trimmed.to_string()
    };
    if let Some(i) = s.find("/ws") {
        s.truncate(i);
    }
    if let Some(i) = s.find("/api/") {
        s.truncate(i);
    }
    s
}

pub async fn signed_post(
    base: &str,
    path: &str,
    keys: &IdentityKeys,
    callsign: &str,
    body: &Value,
) -> Result<Value> {
    let raw = serde_json::to_vec(body)?;
    let sig = hex::encode(keys.sign_bytes(&raw));
    let url = format!("{}{path}", base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let resp = client
        .post(url)
        .header("content-type", "application/json")
        .header("x-wcr-callsign", callsign)
        .header("x-wcr-pubkey", keys.public_hex())
        .header("x-wcr-signature", sig)
        .body(raw)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| Error::Net(e.to_string()))?;
    if !status.is_success() {
        return Err(Error::Net(format!("hub mail {status}: {text}")));
    }
    serde_json::from_str(&text).map_err(|e| Error::Net(e.to_string()))
}

pub fn send_body(from: &str, to: &str, subject: &str, body: &str, bcc: Option<&str>) -> Value {
    json!({
        "from": from,
        "to": to,
        "subject": subject,
        "body": body,
        "bcc": bcc,
    })
}
