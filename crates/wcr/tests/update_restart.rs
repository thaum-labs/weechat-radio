//! Update and restart from the web page, end to end, with no sudo (#280).
//!
//! Each test runs the real `wcr web` (and its station, `wcr node`, in
//! internet mode) from a scratch install folder, offers it a "new version"
//! from a local release list, presses Update on the page's API and checks:
//! the process restarts in place (same PID, nothing asks for a password),
//! the station comes back as it was, the version after the restart is the
//! installed one, the restart waits for an idle radio, and a new version
//! that fails its start is rolled back.
//!
//! The "new version" is a small script that runs this same `wcr` with the
//! test hook `WCR_TEST_FAKE_VERSION=9.9.9`, so no second build is needed.
#![cfg(unix)]

#[path = "common/watchdog.rs"]
mod watchdog;

use axum::routing::get;
use axum::Router;
use serde_json::{json, Value};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const NEW: &str = "9.9.9";
const PASSWORD: &str = "harness-password";

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn asset_name() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    format!("wcr-{os}-{arch}.tar.gz")
}

struct Rig {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    install: PathBuf,
    cfg: PathBuf,
    web_port: u16,
    status_port: u16,
    child: Child,
    http: reqwest::Client,
    cookie: String,
    releases: String,
}

impl Drop for Rig {
    fn drop(&mut self) {
        // SIGTERM ends the station and modem cleanly, then make sure.
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
        std::thread::sleep(Duration::from_millis(500));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The release list, the new package and its SHA-256, on a local port.
async fn serve_releases(root: &Path) -> String {
    let pkg_dir = root.join("pkg");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    let real = root.join("real").join("wcr-real");
    let script = format!(
        "#!/bin/sh\nexport {}={NEW}\nexport {}={}\nexec {} \"$@\"\n",
        wcr::update::TEST_VERSION_ENV,
        wcr::restart::TEST_INSTALL_EXE_ENV,
        root.join("bin").join("wcr").display(),
        real.display()
    );
    std::fs::write(pkg_dir.join("wcr"), script).unwrap();
    chmod_x(&pkg_dir.join("wcr"));
    let tgz = root.join(asset_name());
    let ok = Command::new("tar")
        .arg("-czf")
        .arg(&tgz)
        .arg("-C")
        .arg(&pkg_dir)
        .arg("wcr")
        .status()
        .unwrap()
        .success();
    assert!(ok, "tar");
    let bytes = std::fs::read(&tgz).unwrap();
    let sum = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(&bytes))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let name = asset_name();
    let releases = json!([{
        "tag_name": format!("v{NEW}"),
        "assets": [
            { "name": name, "browser_download_url": format!("{base}/dl/pkg") },
            { "name": "SHA256SUMS.txt", "browser_download_url": format!("{base}/dl/sums") },
        ]
    }]);
    let sums = format!("{sum}  {name}\n");
    let app = Router::new()
        .route(
            "/releases",
            get(move || async move { axum::Json(releases) }),
        )
        .route("/dl/pkg", get(move || async move { bytes }))
        .route("/dl/sums", get(move || async move { sums }));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("{base}/releases")
}

fn chmod_x(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A scratch install with `wcr` in `bin/`, a config, and `wcr web` running.
/// `station`: the saved Start/Stop. `extra`: more wcr.toml.
async fn rig(station: &str, extra: &str) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let built = PathBuf::from(env!("CARGO_BIN_EXE_wcr"));
    for dir in ["bin", "real", "home/config", "home/data"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let install = root.join("bin").join("wcr");
    std::fs::copy(&built, &install).unwrap();
    std::fs::copy(&built, root.join("real").join("wcr-real")).unwrap();
    chmod_x(&install);
    chmod_x(&root.join("real").join("wcr-real"));
    let releases = serve_releases(&root).await;

    let web_port = free_port();
    let status_port = free_port();
    let irc_port = free_port();
    let lan_port = free_port();
    let hash = wcr::web::hash_password(PASSWORD).unwrap();
    let cfg = root.join("home/config/wcr.toml");
    std::fs::write(
        &cfg,
        format!(
            "callsign = \"M0TST\"\ngrid = \"IO91wm\"\nmode = \"internet\"\n\
             [irc]\nbind = \"127.0.0.1:{irc_port}\"\n\
             [status]\nbind = \"127.0.0.1:{status_port}\"\n\
             [hub]\nurl = \"\"\n\
             [telemetry]\nurl = \"\"\n\
             [lan]\ndiscovery = false\nport = {lan_port}\n\
             [store]\npath = \"{}\"\n\
             [web]\nbind = \"127.0.0.1:{web_port}\"\npassword_hash = \"{hash}\"\n\
             {extra}\n",
            root.join("home/data/store.db").display()
        ),
    )
    .unwrap();
    std::fs::write(
        cfg.with_file_name("web-station.state"),
        format!("{station}\n"),
    )
    .unwrap();
    let log = std::fs::File::create(root.join("web.log")).unwrap();
    let child = Command::new(&install)
        .arg("-c")
        .arg(&cfg)
        .arg("web")
        .env("WCR_HOME", root.join("home"))
        .env(wcr::update::TEST_RELEASES_ENV, &releases)
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .env("TMPDIR", &root)
        // No terminal: anything that asked for a password would fail.
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    let http = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut rig = Rig {
        _tmp: tmp,
        root,
        install,
        cfg,
        web_port,
        status_port,
        child,
        http,
        cookie: String::new(),
        releases,
    };
    rig.login().await;
    rig
}

impl Rig {
    fn url(&self, path: &str) -> String {
        format!("https://127.0.0.1:{}{path}", self.web_port)
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.root.join("web.log")).unwrap_or_default()
    }

    async fn login(&mut self) {
        let began = Instant::now();
        loop {
            let res = self
                .http
                .post(self.url("/login"))
                .form(&[("password", PASSWORD)])
                .send()
                .await;
            if let Ok(r) = res {
                if let Some(c) = r.headers().get("set-cookie") {
                    let c = c.to_str().unwrap();
                    self.cookie = c.split(';').next().unwrap().to_string();
                    return;
                }
            }
            assert!(
                began.elapsed() < Duration::from_secs(30),
                "wcr web did not come up:\n{}",
                self.log()
            );
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    async fn get(&self, path: &str) -> Option<Value> {
        let r = self
            .http
            .get(self.url(path))
            .header("cookie", &self.cookie)
            .send()
            .await
            .ok()?;
        assert_ne!(r.status(), 401, "the login was lost across the restart");
        r.json().await.ok()
    }

    async fn post(&self, path: &str, body: Value) -> Value {
        let r = self
            .http
            .post(self.url(path))
            .header("cookie", &self.cookie)
            .json(&body)
            .send()
            .await
            .unwrap();
        r.json().await.unwrap()
    }

    /// Poll `/api/update` until `done` says so.
    async fn wait_update(&self, limit: Duration, done: impl Fn(&Value) -> bool) -> Value {
        let began = Instant::now();
        let mut last = Value::Null;
        while began.elapsed() < limit {
            if let Some(v) = self.get("/api/update").await {
                if done(&v) {
                    return v;
                }
                last = v;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        panic!("timed out; last /api/update: {last}\nlog:\n{}", self.log());
    }

    fn run_file(&self) -> Value {
        let text = std::fs::read_to_string(self.cfg.with_file_name("web-run.json")).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    async fn station_version(&self) -> Option<String> {
        let snap: Value = self
            .http
            .get(format!("http://127.0.0.1:{}/status", self.status_port))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        snap["version"].as_str().map(str::to_string)
    }

    async fn wait_station(&self, want: &str, limit: Duration) {
        let began = Instant::now();
        while began.elapsed() < limit {
            if self.station_version().await.as_deref() == Some(want) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        panic!("station never ran {want}:\n{}", self.log());
    }
}

fn old() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tokio::test(flavor = "multi_thread")]
async fn update_and_restart_from_the_page_needs_no_sudo_and_keeps_the_station() {
    let _guard = watchdog::watchdog(Duration::from_secs(300).min(watchdog::TEST_LIMIT));
    let rig = rig("start", "").await;
    rig.wait_station(old(), Duration::from_secs(30)).await;
    let pid = rig.run_file()["pid"].as_u64().unwrap();

    let offered = rig
        .wait_update(Duration::from_secs(30), |v| v["available"] == NEW)
        .await;
    assert_eq!(offered["running"], old());
    assert_eq!(offered["restart_needed"], false);

    let r = rig.post("/api/update/apply", json!({})).await;
    assert_eq!(r["ok"], true, "{r}");
    // The page answers with the new version within 60 s, logged in still.
    let after = rig
        .wait_update(Duration::from_secs(60), |v| v["running"] == NEW)
        .await;
    assert_eq!(after["installed"], NEW, "{after}");
    assert_eq!(after["restart_needed"], false, "{after}");
    // Restarted in place: same process, so no service manager and no sudo.
    let run = rig.run_file();
    assert_eq!(run["pid"].as_u64().unwrap(), pid);
    assert_eq!(run["version"], NEW);
    // The station was running before, so it runs again, on the new version.
    rig.wait_station(NEW, Duration::from_secs(30)).await;
    // The config was backed up before the update.
    let mut backup = rig.cfg.as_os_str().to_owned();
    backup.push(".before-update");
    assert!(PathBuf::from(backup).is_file());
    // The previous binary is kept beside the new one.
    assert!(rig.install.with_extension("bak").is_file());
    let log = rig.log();
    assert!(log.contains("radio idle"), "{log}");
    assert!(!log.to_lowercase().contains("sudo"), "{log}");
}

#[tokio::test(flavor = "multi_thread")]
async fn waits_for_idle_radio_before_restart() {
    let _guard = watchdog::watchdog(Duration::from_secs(300).min(watchdog::TEST_LIMIT));
    // The station stays stopped; a stand-in answers its status port and
    // says the radio is sending until `busy` is cleared.
    let rig = rig("stop", "").await;
    let busy = Arc::new(AtomicBool::new(true));
    let flag = busy.clone();
    let app = Router::new().route(
        "/status",
        get(move || {
            let flag = flag.clone();
            async move {
                let s = wcr::status::StatusSnapshot {
                    ptt_on: flag.load(Ordering::SeqCst),
                    channel: "idle".into(),
                    ..Default::default()
                };
                axum::Json(s)
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", rig.status_port))
        .await
        .unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let started = rig.run_file()["started"].as_u64().unwrap();
    rig.wait_update(Duration::from_secs(30), |v| v["available"] == NEW)
        .await;
    rig.post("/api/update/apply", json!({})).await;
    let waiting = rig
        .wait_update(Duration::from_secs(60), |v| {
            v["phase"]
                .as_str()
                .is_some_and(|p| p.contains("Waiting for the radio to go quiet (sending)"))
        })
        .await;
    assert_eq!(waiting["running"], old());
    // Held while the radio sends: nothing restarts.
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_eq!(rig.run_file()["started"].as_u64().unwrap(), started);
    let still = rig.get("/api/update").await.unwrap();
    assert_eq!(still["running"], old(), "{still}");
    assert_eq!(still["installed"], NEW, "{still}");
    assert_eq!(still["restart_needed"], true, "{still}");

    busy.store(false, Ordering::SeqCst);
    let after = rig
        .wait_update(Duration::from_secs(60), |v| v["running"] == NEW)
        .await;
    assert_eq!(after["installed"], NEW);
    // The station was stopped, so it stays stopped.
    let log = rig.log();
    assert!(
        log.contains("station not started: it was stopped from the web page"),
        "{log}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rolls_back_on_failed_start() {
    let _guard = watchdog::watchdog(Duration::from_secs(300).min(watchdog::TEST_LIMIT));
    let rig = rig("start", "[update]\ntest_versions = true\n").await;
    rig.wait_station(old(), Duration::from_secs(30)).await;
    let v = rig
        .wait_update(Duration::from_secs(30), |v| v["available"] == NEW)
        .await;
    assert_eq!(v["test_versions"], true);
    rig.post("/api/update/apply", json!({ "simulate_failure": true }))
        .await;
    let after = rig
        .wait_update(Duration::from_secs(90), |v| {
            v["note"]
                .as_str()
                .is_some_and(|n| n.starts_with("Rolled back"))
        })
        .await;
    // Back on the previous version, and the page says so.
    assert_eq!(after["running"], old(), "{after}");
    assert_eq!(after["installed"], old(), "{after}");
    assert!(
        after["note"]
            .as_str()
            .unwrap()
            .contains(&format!("Rolled back to {}: {NEW} did not start", old())),
        "{after}"
    );
    // The station comes back as it was (running), on the previous version.
    rig.wait_station(old(), Duration::from_secs(30)).await;
    let out = Command::new(&rig.install)
        .arg("--version")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains(old()));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cli_updates_restarts_and_says_so_in_one_line() {
    let _guard = watchdog::watchdog(Duration::from_secs(300).min(watchdog::TEST_LIMIT));
    let rig = rig("start", "").await;
    rig.wait_station(old(), Duration::from_secs(30)).await;
    let pid = rig.run_file()["pid"].as_u64().unwrap();
    let cli = |args: &[&str]| {
        let mut cmd = tokio::process::Command::new(&rig.install);
        cmd.arg("-c")
            .arg(&rig.cfg)
            .args(args)
            .env("WCR_HOME", rig.root.join("home"))
            .env(wcr::update::TEST_RELEASES_ENV, &rig.releases)
            .env("NO_PROXY", "127.0.0.1,localhost")
            .env("TMPDIR", &rig.root)
            .stdin(Stdio::null());
        cmd
    };
    let out = cli(&["update"]).output().await.unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "{text}\n{}\n{}",
        String::from_utf8_lossy(&out.stderr),
        rig.log()
    );
    let last = text.lines().last().unwrap_or("");
    assert_eq!(
        last,
        format!("Updated to {NEW}, running, radio idle."),
        "{text}"
    );
    assert_eq!(rig.run_file()["pid"].as_u64().unwrap(), pid);
    assert_eq!(rig.station_version().await.as_deref(), Some(NEW));

    let out = cli(&["status"]).output().await.unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains(&format!("Installed: {NEW}")), "{text}");
    assert!(
        text.contains(&format!("Web page:  running {NEW}")),
        "{text}"
    );
    assert!(
        text.contains(&format!("Station:   running {NEW}")),
        "{text}"
    );

    // `wcr restart` (no sudo) restarts in place on the same version.
    let started = rig.run_file()["started"].as_u64().unwrap();
    let out = cli(&["restart"]).output().await.unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "{text}");
    assert_eq!(
        text.lines().last().unwrap_or(""),
        format!("Restarted on {NEW}, running, radio idle."),
        "{text}"
    );
    let run = rig.run_file();
    assert_eq!(run["pid"].as_u64().unwrap(), pid);
    assert!(run["started"].as_u64().unwrap() >= started);
}
