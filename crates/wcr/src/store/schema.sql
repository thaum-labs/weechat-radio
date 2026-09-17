-- SPDX-License-Identifier: Apache-2.0
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    msg_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    origin TEXT NOT NULL,
    dest TEXT NOT NULL,
    flags INTEGER NOT NULL,
    hops_left INTEGER NOT NULL,
    ts INTEGER NOT NULL,
    seq INTEGER NOT NULL,
    body BLOB NOT NULL,
    signature BLOB,
    rx_time INTEGER NOT NULL,
    delivery TEXT NOT NULL DEFAULT 'queued',
    is_group INTEGER NOT NULL DEFAULT 0,
    hold_until INTEGER NOT NULL DEFAULT 0,
    retries INTEGER NOT NULL DEFAULT 0,
    suppressed INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_messages_rx ON messages(rx_time);
CREATE INDEX IF NOT EXISTS idx_messages_dest ON messages(dest, rx_time);
CREATE INDEX IF NOT EXISTS idx_messages_origin ON messages(origin, seq);

CREATE TABLE IF NOT EXISTS seen (
    msg_id TEXT PRIMARY KEY,
    first_seen INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,
    hear_count INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS groups (
    name TEXT PRIMARY KEY,
    closed INTEGER NOT NULL DEFAULT 1,
    created INTEGER NOT NULL,
    default_prio INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS group_members (
    group_name TEXT NOT NULL,
    callsign TEXT NOT NULL,
    PRIMARY KEY (group_name, callsign)
);

CREATE TABLE IF NOT EXISTS group_receipts (
    group_name TEXT NOT NULL,
    msg_id TEXT NOT NULL,
    callsign TEXT NOT NULL,
    seen_at INTEGER NOT NULL,
    PRIMARY KEY (group_name, msg_id, callsign)
);

CREATE TABLE IF NOT EXISTS heard (
    callsign TEXT PRIMARY KEY,
    last_heard INTEGER NOT NULL,
    snr REAL,
    mode TEXT,
    grid TEXT,
    gateway INTEGER NOT NULL DEFAULT 0,
    medium TEXT NOT NULL DEFAULT 'rf',
    freq_khz INTEGER NOT NULL DEFAULT 0,
    band TEXT
);

CREATE TABLE IF NOT EXISTS hops (
    msg_id TEXT NOT NULL,
    hop_call TEXT NOT NULL,
    medium TEXT NOT NULL,
    ts INTEGER NOT NULL,
    snr REAL,
    PRIMARY KEY (msg_id, hop_call, ts)
);

CREATE TABLE IF NOT EXISTS seq (
    callsign TEXT PRIMARY KEY,
    next_seq INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS mutes (
    target TEXT PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS checkins (
    callsign TEXT PRIMARY KEY,
    note TEXT,
    at INTEGER NOT NULL,
    grid TEXT,
    snr REAL
);

CREATE TABLE IF NOT EXISTS welfare (
    callsign TEXT PRIMARY KEY,
    code TEXT NOT NULL,
    at INTEGER NOT NULL
);
