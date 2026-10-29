//! SPDX-License-Identifier: Apache-2.0
//! HTTPS for the web page: the certificate (a self-signed one is made on
//! first start) and a listener that hands axum finished TLS connections.

use crate::error::{Error, Result};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

/// A browser that has not finished the TLS handshake by now is dropped.
const HANDSHAKE_WAIT: Duration = Duration::from_secs(10);

/// The certificate and key files: the ones in `[web]`, or the self-signed
/// pair in the data folder.
pub fn cert_paths(cfg: &crate::config::WebConfig) -> (PathBuf, PathBuf) {
    if !cfg.cert.trim().is_empty() && !cfg.key.trim().is_empty() {
        return (
            PathBuf::from(cfg.cert.trim()),
            PathBuf::from(cfg.key.trim()),
        );
    }
    let dir = crate::config::default_data_dir().join("web");
    (dir.join("cert.pem"), dir.join("key.pem"))
}

/// Make a self-signed certificate for this computer's names when there is
/// none yet. Returns true when it made one.
pub fn ensure_self_signed(cert: &Path, key: &Path) -> Result<bool> {
    if cert.exists() && key.exists() {
        return Ok(false);
    }
    let host = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "wcr".into());
    let names = vec![host.clone(), format!("{host}.local"), "localhost".into()];
    let made = rcgen::generate_simple_self_signed(names).map_err(|e| Error::Msg(e.to_string()))?;
    if let Some(dir) = cert.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(cert, made.cert.pem())?;
    write_private(key, &made.key_pair.serialize_pem())?;
    Ok(true)
}

#[cfg(unix)]
fn write_private(path: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(text.as_bytes())?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text)?;
    Ok(())
}

pub fn server_config(cert: &Path, key: &Path) -> Result<Arc<ServerConfig>> {
    let bad = |what: &str, e: &dyn std::fmt::Display| {
        Error::config(format!("{what} {}: {e}", cert.display()))
    };
    let certs = CertificateDer::pem_file_iter(cert)
        .map_err(|e| bad("certificate", &e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| bad("certificate", &e))?;
    let key = PrivateKeyDer::from_pem_file(key).map_err(|e| bad("key for", &e))?;
    let mut config = ServerConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| Error::Msg(e.to_string()))?
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .map_err(|e| Error::config(e.to_string()))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// Accepts TCP, finishes each TLS handshake on its own task (a slow or
/// silent browser cannot hold up the others), and hands axum the result.
pub struct TlsListener {
    ready: mpsc::Receiver<(TlsStream<tokio::net::TcpStream>, SocketAddr)>,
    local: SocketAddr,
}

impl TlsListener {
    pub fn new(tcp: TcpListener, config: Arc<ServerConfig>) -> std::io::Result<Self> {
        let local = tcp.local_addr()?;
        let (tx, ready) = mpsc::channel(32);
        let acceptor = TlsAcceptor::from(config);
        tokio::spawn(async move {
            loop {
                let Ok((stream, peer)) = tcp.accept().await else {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                };
                let acceptor = acceptor.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let shake = tokio::time::timeout(HANDSHAKE_WAIT, acceptor.accept(stream));
                    if let Ok(Ok(tls)) = shake.await {
                        let _ = tx.send((tls, peer)).await;
                    }
                });
            }
        });
        Ok(Self { ready, local })
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(conn) => conn,
            // The accept task ended: nothing more will ever arrive.
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_self_signed_pair_is_made_once_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        let (cert, key) = (dir.path().join("cert.pem"), dir.path().join("key.pem"));
        assert!(ensure_self_signed(&cert, &key).unwrap());
        assert!(!ensure_self_signed(&cert, &key).unwrap());
        server_config(&cert, &key).unwrap();
    }
}
