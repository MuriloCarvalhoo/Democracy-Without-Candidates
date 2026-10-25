//! E2E harness: isolated app process (disposable database, free loopback port),
//! chromedriver + headless Chrome, bounded waits and cleanup of spawned processes only.
#![allow(dead_code)]

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use fantoccini::{Client, ClientBuilder, Locator};

pub const WAIT: Duration = Duration::from_secs(20);

/// `App::start` extra env key: path of a file receiving the server's stderr.
pub const STDERR_FILE: &str = "E2E_STDERR_FILE";

pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn http_get_status(port: u16, path: &str) -> Option<u16> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().ok()?,
        Duration::from_millis(500),
    )
    .ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut buf = [0u8; 64];
    let n = stream.read(&mut buf).ok()?;
    std::str::from_utf8(&buf[..n])
        .ok()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn wait_until(what: &str, timeout: Duration, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out after {timeout:?} waiting for {what}");
}

pub struct App {
    child: Child,
    pub port: u16,
    pub db_path: PathBuf,
    env: Vec<(String, String)>,
}

impl App {
    /// Starts the built server binary (`cargo leptos build` first) on a free port.
    pub fn start(db_path: &Path, extra_env: &[(&str, &str)]) -> Self {
        let env: Vec<(String, String)> = extra_env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let port = free_port();
        let app = Self {
            child: spawn_app(db_path, port, &env),
            port,
            db_path: db_path.to_owned(),
            env,
        };
        app.wait_ready();
        app
    }

    fn wait_ready(&self) {
        let port = self.port;
        wait_until("app /healthz", Duration::from_secs(90), || {
            http_get_status(port, "/healthz") == Some(200)
        });
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    /// Stops the process (SIGKILL) and starts it again on the same database and port.
    pub fn restart(&mut self) {
        self.stop();
        self.resume();
    }

    /// Stops the process (SIGKILL) so the test may edit the database; then `resume`.
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    pub fn resume(&mut self) {
        self.child = spawn_app(&self.db_path, self.port, &self.env);
        self.wait_ready();
    }
}

/// Runs `dwc seed-dev` (synthetic fixtures) against a database before starting the app.
pub fn seed_dev(db_path: &Path) {
    let status = app_command(db_path, free_port(), &[])
        .arg("seed-dev")
        .stdout(Stdio::null())
        .status()
        .expect("run seed-dev");
    assert!(status.success(), "seed-dev failed: {status}");
}

/// Plain HTTP GET returning (status, body) for non-browser checks (downloads).
pub fn http_get(port: u16, path: &str) -> (u16, String) {
    http_get_with(port, path, &[])
}

/// Plain HTTP GET with extra headers (e.g. a session cookie), returning (status, body).
pub fn http_get_with(port: u16, path: &str, headers: &[(&str, &str)]) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let extra: String = headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}\r\n"))
        .collect();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n{extra}\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("read response");
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_owned())
        .unwrap_or_default();
    (status, body)
}

/// Plain HTTP POST with extra headers, returning (status, body).
pub fn http_post(port: u16, path: &str, headers: &[(&str, &str)], body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let extra: String = headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}\r\n"))
        .collect();
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {}\r\n{extra}\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("read response");
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_owned())
        .unwrap_or_default();
    (status, body)
}

fn spawn_app(db_path: &Path, port: u16, env: &[(String, String)]) -> Child {
    let mut child = app_command(db_path, port, env).spawn().expect("spawn app");
    // Fail fast if the process exits during startup (bad config, port in use).
    std::thread::sleep(Duration::from_millis(50));
    if let Ok(Some(status)) = child.try_wait() {
        panic!("app exited during startup: {status}");
    }
    child
}

fn app_command(db_path: &Path, port: u16, env: &[(String, String)]) -> Command {
    let bin = std::env::var("DWC_E2E_BIN").unwrap_or_else(|_| "target/e2e/dwc".into());
    assert!(
        Path::new(&bin).exists(),
        "{bin} not found: run `just e2e` (builds with cargo-leptos and copies the binary)"
    );
    let mut cmd = Command::new(&bin);
    cmd.env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("RUST_LOG", "warn")
        .env("DWC_ENV", "test")
        .env("DWC_DB_PATH", db_path)
        .env("DWC_COUNTRY", "BR")
        .env("DWC_IDENTITY_PROVIDER", "synthetic")
        .env("LEPTOS_SITE_ROOT", "target/site")
        .env("LEPTOS_OUTPUT_NAME", "dwc")
        .env("LEPTOS_SITE_ADDR", format!("127.0.0.1:{port}"))
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    for (k, v) in env {
        // Harness option, not an app setting: append the server log to a file.
        if k == STDERR_FILE {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(v)
                .expect("server log file");
            cmd.stderr(file);
        } else {
            cmd.env(k, v);
        }
    }
    cmd
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Browser {
    driver: Child,
    pub client: Client,
}

impl Browser {
    pub async fn start() -> Self {
        let port = free_port();
        let bin = std::env::var("CHROMEDRIVER").unwrap_or_else(|_| "chromedriver".into());
        let driver = Command::new(&bin)
            .arg(format!("--port={port}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("chromedriver ({bin}) is required for E2E: {e}"));
        wait_until("chromedriver", Duration::from_secs(20), || {
            http_get_status(port, "/status") == Some(200)
        });

        let mut chrome = serde_json::json!({
            "args": ["--headless=new", "--no-sandbox", "--disable-gpu", "--window-size=390,844", "--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"]
        });
        if let Ok(binary) = std::env::var("CHROME_BIN") {
            chrome["binary"] = binary.into();
        }
        let mut caps = serde_json::Map::new();
        caps.insert("goog:chromeOptions".into(), chrome);
        // Browser console entries, read back with `Browser::console`.
        caps.insert(
            "goog:loggingPrefs".into(),
            serde_json::json!({ "browser": "ALL" }),
        );
        let client = ClientBuilder::rustls()
            .expect("tls")
            .capabilities(caps)
            .connect(&format!("http://127.0.0.1:{port}"))
            .await
            .expect("webdriver session");
        Self { driver, client }
    }

    /// SEVERE browser console entries since the last call. Drains the console,
    /// so it also fails on app crashes that `close` would no longer see.
    pub async fn console_errors(&self) -> Vec<String> {
        let severe: Vec<String> = self
            .console()
            .await
            .into_iter()
            .filter_map(|l| l.strip_prefix("SEVERE ").map(str::to_owned))
            .collect();
        let crashes: Vec<&String> = severe
            .iter()
            .filter(|l| {
                l.contains("panicked")
                    || l.contains("hydration error")
                    || l.contains("RuntimeError")
            })
            .collect();
        assert!(crashes.is_empty(), "browser app crashed: {crashes:#?}");
        severe
    }

    pub async fn goto_hydrated(&self, url: &str) {
        self.client.goto(url).await.expect("goto");
        self.client
            .wait()
            .at_most(WAIT)
            .for_element(Locator::Css("main[data-hydrated='true']"))
            .await
            .expect("page hydrated");
    }

    pub async fn text_of(&self, css: &str) -> String {
        self.client
            .find(Locator::Css(css))
            .await
            .expect(css)
            .text()
            .await
            .unwrap()
    }

    /// Polls until the element text satisfies `pred` (bounded).
    pub async fn wait_text(&self, css: &str, pred: impl Fn(&str) -> bool) -> String {
        let start = Instant::now();
        loop {
            if let Ok(el) = self.client.find(Locator::Css(css)).await
                && let Ok(text) = el.text().await
                && pred(&text)
            {
                return text;
            }
            assert!(
                start.elapsed() < WAIT,
                "timed out waiting for text in {css}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Browser console entries since the previous call, as `LEVEL message`
    /// (chromedriver log endpoint; requires `goog:loggingPrefs`).
    pub async fn console(&self) -> Vec<String> {
        let Ok(value) = self.client.issue_cmd(BrowserLog).await else {
            return Vec::new();
        };
        value
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| {
                        format!(
                            "{} {}",
                            e["level"].as_str().unwrap_or("?"),
                            e["message"].as_str().unwrap_or("")
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Console entries meaning the WebAssembly app stopped (panic, hydration
    /// failure, trap). Drains the console like `console`.
    pub async fn crashes(&self) -> Vec<String> {
        self.console()
            .await
            .into_iter()
            .filter(|l| {
                l.contains("panicked")
                    || l.contains("hydration error")
                    || l.contains("RuntimeError")
            })
            .collect()
    }

    /// Ends the session; fails the test if the app panicked since the last
    /// console read (a browser panic stops the whole app).
    pub async fn close(self) {
        let crashes = self.crashes().await;
        self.close_allowing_crash().await;
        assert!(crashes.is_empty(), "browser app crashed: {crashes:#?}");
    }

    /// For tests that trigger a panic on purpose.
    pub async fn close_allowing_crash(self) {
        let _ = self.client.close().await;
        let mut driver = self.driver;
        let _ = driver.kill();
        let _ = driver.wait();
    }
}

/// chromedriver `POST /session/{id}/se/log` for the browser console.
#[derive(Debug)]
struct BrowserLog;

impl fantoccini::wd::WebDriverCompatibleCommand for BrowserLog {
    fn endpoint(
        &self,
        base_url: &url::Url,
        session_id: Option<&str>,
    ) -> Result<url::Url, url::ParseError> {
        base_url.join(&format!(
            "session/{}/se/log",
            session_id.unwrap_or_default()
        ))
    }

    fn method_and_body(&self, _: &url::Url) -> (axum::http::Method, Option<String>) {
        (
            axum::http::Method::POST,
            Some(r#"{"type":"browser"}"#.to_owned()),
        )
    }
}

/// Synthetic active account with the given IAM policy (id) and a live session (no
/// login UI: the session row is created directly). Returns the cookie token.
pub async fn account_with_session(
    db: &dwc::infrastructure::db::Db,
    email: &str,
    policy: &str,
) -> (String, String) {
    use dwc::{
        application::session::{self, SessionPolicy},
        infrastructure::db::query_opt,
    };
    let now = chrono::Utc::now();
    let tx = db.write().await.unwrap();
    let row = query_opt(
        &tx,
        "SELECT m.id, s.id, s.parent_id FROM jurisdictions m JOIN jurisdictions s ON s.id = m.parent_id WHERE m.code = '4202404'",
        (),
    )
    .await
    .unwrap()
    .unwrap();
    let (m, s, c): (String, String, String) = (
        row.get(0).unwrap(),
        row.get(1).unwrap(),
        row.get(2).unwrap(),
    );
    let id = format!("user-{policy}");
    tx.execute(
        "INSERT INTO users (id, email, name, password_hash, status, municipality_id, state_id, country_id, synthetic, created_at, updated_at)
         VALUES (?1, ?2, 'Pessoa Sintética', 'x', 'active', ?3, ?4, ?5, 1, 'now', 'now')",
        (id.as_str(), email, m, s, c),
    )
    .await
    .unwrap();
    tx.execute(
        "INSERT INTO iam_user_policies (user_id, policy_id, attached_at) VALUES (?1, ?2, 'now')",
        (id.as_str(), policy),
    )
    .await
    .unwrap();
    let token = session::create(&tx, now, SessionPolicy::from_secs(900, 28800), &id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (id, token)
}

/// Live session for an existing (seeded) account, created directly. Returns the token.
pub async fn session_for(db: &dwc::infrastructure::db::Db, email: &str) -> String {
    use dwc::{
        application::session::{self, SessionPolicy},
        infrastructure::db::query_opt,
    };
    let tx = db.write().await.unwrap();
    let row = query_opt(&tx, "SELECT id FROM users WHERE email = ?1", [email])
        .await
        .unwrap()
        .expect(email);
    let id: String = row.get(0).unwrap();
    let token = session::create(
        &tx,
        chrono::Utc::now(),
        SessionPolicy::from_secs(900, 28800),
        &id,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    token
}

/// Installs a session cookie in the browser (test-only: the real cookie is HttpOnly).
pub async fn use_session(browser: &Browser, app: &App, token: &str) {
    browser.client.goto(&app.url("/healthz")).await.unwrap();
    browser.client.delete_all_cookies().await.unwrap();
    browser
        .client
        .execute(
            "document.cookie = 'dwc_session=' + arguments[0] + '; path=/; SameSite=Lax';",
            vec![token.into()],
        )
        .await
        .unwrap();
}
