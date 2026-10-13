//! SPDX-License-Identifier: Apache-2.0
//! Reject DNS-rebinding attacks against loopback-only HTTP APIs.

use axum::body::Body;
use axum::http::{header::HOST, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

pub fn host_allowed(host: &str) -> bool {
    let host = host.trim();
    let h = if host.starts_with('[') {
        host.split(']')
            .next()
            .unwrap_or(host)
            .trim_start_matches('[')
    } else {
        host.split(':').next().unwrap_or(host)
    };
    h.eq_ignore_ascii_case("localhost") || h == "127.0.0.1" || h == "::1"
}

pub async fn require_loopback_host(request: Request<Body>, next: Next) -> Response {
    let ok = request
        .headers()
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .map(host_allowed)
        .unwrap_or(true);
    if ok {
        next.run(request).await
    } else {
        Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::from("forbidden"))
            .unwrap_or_else(|_| Response::new(Body::empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_loopback_hosts() {
        assert!(host_allowed("127.0.0.1:8074"));
        assert!(host_allowed("localhost"));
        assert!(host_allowed("[::1]:8074"));
        assert!(!host_allowed("evil.example"));
    }
}
