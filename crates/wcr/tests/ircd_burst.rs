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

fn privmsg_text(line: &str) -> Option<&str> {
    let line = line.trim_end_matches(['\r', '\n']);
    let payload = if let Some(rest) = line.strip_prefix('@') {
        rest.split_once(' ')?.1
    } else {
        line
    };
    payload
        .split_once("PRIVMSG ")?
        .1
        .split_once(" :")
        .map(|(_, b)| b)
}

async fn collect_echo_bodies(raw: &str) -> Vec<String> {
    let (events_tx, _events_rx) = mpsc::channel(8);
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
    stream.write_all(reg.as_bytes()).await.expect("reg");
    stream.write_all(raw.as_bytes()).await.expect("write");
    stream.flush().await.expect("flush");
    let expect = raw.matches("PRIVMSG ").count();
    let mut reader = tokio::io::BufReader::new(stream);
    let mut bodies = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut line_buf = String::new();
    while tokio::time::Instant::now() < deadline && bodies.len() < expect {
        line_buf.clear();
        let read = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            reader.read_line(&mut line_buf),
        )
        .await;
        match read {
            Ok(Ok(0)) => break,
            Ok(Err(e)) => panic!("read: {e}"),
            Err(_) => continue,
            Ok(Ok(_)) => {}
        }
        if let Some(body) = privmsg_text(&line_buf) {
            bodies.push(body.to_string());
        }
    }
    bodies
}

#[tokio::test]
async fn privmsg_body_exact_crlf_lf_and_burst() {
    let one = collect_echo_bodies("PRIVMSG #bulletin :solo-body\r\n").await;
    assert_eq!(one, vec!["solo-body".to_string()]);

    let lf = collect_echo_bodies("PRIVMSG #bulletin :lf-only\n").await;
    assert_eq!(lf, vec!["lf-only".to_string()]);

    let burst = collect_echo_bodies(concat!(
        "PRIVMSG #bulletin :one\r\n",
        "PRIVMSG #bulletin :two\n",
        "PRIVMSG #bulletin :three\r\n",
    ))
    .await;
    assert_eq!(
        burst,
        vec!["one".to_string(), "two".to_string(), "three".to_string()]
    );
}

#[tokio::test]
async fn mode_help_and_invite_get_replies() {
    let (events_tx, mut events_rx) = mpsc::channel(32);
    let server = IrcServer::new(events_tx);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = server.accept_loop(listener).await;
    });
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let burst = concat!(
        "NICK G4ABC\r\n",
        "USER G4ABC 0 * :wcr test\r\n",
        "MODE #ops\r\n",
        "MODE #bulletin\r\n",
        "MODE #ops +i\r\n",
        "HELP\r\n",
        "INVITE M0XYZ #ops\r\n",
        "INVITE M0XYZ #bulletin\r\n",
    );
    stream.write_all(burst.as_bytes()).await.expect("write");
    stream.flush().await.expect("flush");

    let mut reader = tokio::io::BufReader::new(stream);
    let mut lines = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut line_buf = String::new();
    while tokio::time::Instant::now() < deadline {
        line_buf.clear();
        let read = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            reader.read_line(&mut line_buf),
        )
        .await;
        match read {
            Ok(Ok(0)) => break,
            Ok(Err(e)) => panic!("read: {e}"),
            Err(_) => {
                if lines.contains(" 706 ") && lines.matches(" 341 ").count() >= 2 {
                    break;
                }
                continue;
            }
            Ok(Ok(_)) => lines.push_str(&line_buf),
        }
    }
    assert!(lines.contains(" 324 "), "mode query: {lines}");
    assert!(lines.contains("#ops +i"), "closed group: {lines}");
    assert!(lines.contains("#bulletin +"), "bulletin mode: {lines}");
    assert!(
        lines.contains("Channel mode stays +i"),
        "set mode notice: {lines}"
    );
    assert!(lines.contains(" 705 "), "help: {lines}");
    assert!(lines.contains("/invite CALL"), "help lists invite: {lines}");
    assert!(
        !lines.contains(" 421 "),
        "help must not be unknown: {lines}"
    );
    assert_eq!(lines.matches(" 341 ").count(), 2, "invite: {lines}");

    let mut invites = 0;
    while let Ok(ev) = events_rx.try_recv() {
        if matches!(ev.kind, wcr::ircd::IrcEventKind::Invite { .. }) {
            invites += 1;
        }
    }
    assert_eq!(invites, 2);
}

#[tokio::test]
async fn delivery_mark_goes_only_to_the_sending_client() {
    let (events_tx, _events_rx) = mpsc::channel(8);
    let server = IrcServer::new(events_tx);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let accept = server.clone();
    tokio::spawn(async move {
        let _ = accept.accept_loop(listener).await;
    });

    let mut sender = TcpStream::connect(addr).await.expect("sender");
    sender
        .write_all(b"CAP REQ :message-tags\r\nNICK G4AAA\r\nUSER G4AAA 0 * :t\r\nCAP END\r\n")
        .await
        .expect("write");
    let mut id = None;
    for _ in 0..50 {
        id = server.first_client_id();
        if id.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let id = id.expect("sender connected");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    server.begin_origin(id, "#ops");
    server.tagmsg_progress("abc123", "sent", Some(1)).await;
    server.end_origin();

    let mut other = TcpStream::connect(addr).await.expect("other");
    other
        .write_all(b"CAP REQ :message-tags\r\nNICK M0BBB\r\nUSER M0BBB 0 * :t\r\nCAP END\r\n")
        .await
        .expect("write");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    server.tagmsg_delivery("abc123", "delivered").await;
    server.tagmsg_delivery("someoneelse", "all").await;

    let mut sender_buf = String::new();
    let mut reader = tokio::io::BufReader::new(sender);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let mut line = String::new();
        match tokio::time::timeout(
            std::time::Duration::from_millis(200),
            reader.read_line(&mut line),
        )
        .await
        {
            Err(_) => continue,
            Ok(Ok(0)) => break,
            Ok(Ok(_)) => sender_buf.push_str(&line),
            Ok(Err(e)) => panic!("read sender: {e}"),
        }
    }
    assert!(
        sender_buf.contains("msgid=abc123"),
        "mark keeps the id: {sender_buf}"
    );
    assert!(
        sender_buf.contains("TAGMSG #ops"),
        "mark names the channel: {sender_buf}"
    );
    assert!(
        sender_buf.contains("+radio/delivery=sent"),
        "first mark: {sender_buf}"
    );
    assert!(
        sender_buf.contains("+radio/delivery=delivered"),
        "later mark still reaches the sender: {sender_buf}"
    );
    assert!(
        !sender_buf.contains("someoneelse"),
        "another station's mark is not delivered: {sender_buf}"
    );

    let mut other_buf = String::new();
    let mut other_reader = tokio::io::BufReader::new(other);
    let other_deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(400);
    while tokio::time::Instant::now() < other_deadline {
        let mut line = String::new();
        match tokio::time::timeout(
            std::time::Duration::from_millis(150),
            other_reader.read_line(&mut line),
        )
        .await
        {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(_)) => other_buf.push_str(&line),
            Ok(Err(e)) => panic!("read other: {e}"),
        }
    }
    assert!(
        !other_buf.contains("TAGMSG"),
        "other client must not see delivery marks: {other_buf}"
    );
}
