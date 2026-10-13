//! SPDX-License-Identifier: Apache-2.0
//! Registration must survive a single TCP write with CAP/NICK/USER/… burst.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use wcr::ircd::IrcServer;

#[tokio::test]
async fn registration_burst_gets_001_and_privmsg_echo() {
    let (events_tx, _events_rx) = mpsc::channel(32);
    let server = IrcServer::new(events_tx);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = server.accept_loop(listener).await;
    });

    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let burst = concat!(
        "CAP LS\r\n",
        "NICK G4ABC\r\n",
        "USER G4ABC 0 * :wcr test\r\n",
        "CAP REQ :message-tags echo-message server-time msgid\r\n",
        "CAP END\r\n",
        "JOIN #bulletin\r\n",
        "PRIVMSG #bulletin :one\r\n",
        "PRIVMSG #bulletin :two\r\n",
        "PRIVMSG #bulletin :three\r\n",
    );
    stream.write_all(burst.as_bytes()).await.expect("write");
    stream.flush().await.expect("flush");

    let mut reader = tokio::io::BufReader::new(stream);
    let mut got_001 = false;
    let mut privmsg_echoes = 0u32;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut line_buf = String::new();
    while tokio::time::Instant::now() < deadline {
        line_buf.clear();
        let read = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            reader.read_line(&mut line_buf),
        )
        .await;
        match read {
            Ok(Ok(0)) => break,
            Ok(Ok(_)) => {}
            Ok(Err(e)) => panic!("read: {e}"),
            Err(_) => continue,
        }
        let n = line_buf.as_str();
        if n.trim().is_empty() {
            continue;
        }
        if n.contains(" 001 ") {
            got_001 = true;
        }
        if n.contains("PRIVMSG #bulletin :one")
            || n.contains("PRIVMSG #bulletin :two")
            || n.contains("PRIVMSG #bulletin :three")
        {
            privmsg_echoes += 1;
        }
        if got_001 && privmsg_echoes >= 3 {
            break;
        }
    }
    assert!(got_001, "expected IRC 001 welcome after registration burst");
    assert!(
        privmsg_echoes >= 3,
        "expected echo-message copies for three PRIVMSGs, got {privmsg_echoes}"
    );
}

#[tokio::test]
async fn flood_limit_notices_but_registration_survives() {
    let (events_tx, _events_rx) = mpsc::channel(32);
    let server = IrcServer::new(events_tx);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = server.accept_loop(listener).await;
    });

    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let reg = concat!(
        "CAP LS\r\n",
        "NICK G4ABC\r\n",
        "USER G4ABC 0 * :wcr test\r\n",
        "CAP REQ :message-tags echo-message server-time msgid\r\n",
        "CAP END\r\n",
        "JOIN #bulletin\r\n",
    );
    stream.write_all(reg.as_bytes()).await.expect("write");
    let flood = "PRIVMSG #bulletin :flood\r\n".repeat(120);
    stream.write_all(flood.as_bytes()).await.expect("flood");
    stream.flush().await.expect("flush");

    let mut reader = tokio::io::BufReader::new(stream);
    let mut got_001 = false;
    let mut notices = 0u32;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut line_buf = String::new();
    while tokio::time::Instant::now() < deadline {
        line_buf.clear();
        let read = tokio::time::timeout(
            std::time::Duration::from_millis(400),
            reader.read_line(&mut line_buf),
        )
        .await;
        match read {
            Ok(Ok(0)) => break,
            Ok(Ok(_)) => {}
            Ok(Err(e)) => panic!("read: {e}"),
            Err(_) => continue,
        }
        let n = line_buf.as_str();
        if n.contains(" 001 ") {
            got_001 = true;
        }
        if n.contains(" NOTICE ") && n.contains("rate limited") {
            notices += 1;
        }
        if got_001 && notices > 0 {
            break;
        }
    }
    assert!(got_001, "registration must complete under flood");
    assert!(notices > 0, "expected at least one flood NOTICE");
}
