//! One HTTP client for every source: polite per-host spacing, timeouts and retry with backoff.

use crate::error::{AppError, AppResult};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";
const MIN_SPACING: Duration = Duration::from_millis(120);
const API_TIMEOUT: Duration = Duration::from_secs(15);
const RETRIES: u32 = 2;

pub struct Http {
    pub client: Client,
    last_hit: Mutex<HashMap<String, Instant>>,
}

impl Http {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(8))
            .read_timeout(Duration::from_secs(20))
            .gzip(true)
            .build()
            .expect("TLS backend is compiled in");
        Self { client, last_hit: Mutex::new(HashMap::new()) }
    }

    /// Waits so the same host is never hit more often than every MIN_SPACING.
    async fn pace(&self, url: &str) {
        let host = url.split('/').nth(2).unwrap_or("").to_string();
        let wait = {
            let mut map = self.last_hit.lock().unwrap_or_else(|p| p.into_inner());
            let now = Instant::now();
            let next = map.get(&host).map(|t| *t + MIN_SPACING).unwrap_or(now).max(now);
            map.insert(host, next);
            next - now
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    /// Sends a request built by `build`, retrying on timeouts, 429 and 5xx with backoff.
    pub async fn send(&self, url: &str, build: impl Fn(&Client) -> RequestBuilder) -> AppResult<Response> {
        let mut attempt = 0;
        loop {
            self.pace(url).await;
            let result = build(&self.client).timeout(API_TIMEOUT).send().await;
            let retryable = match &result {
                Ok(r) => r.status() == StatusCode::TOO_MANY_REQUESTS || r.status().is_server_error(),
                Err(e) => e.is_timeout() || e.is_connect(),
            };
            if !retryable || attempt >= RETRIES {
                return Ok(result?);
            }
            attempt += 1;
            tokio::time::sleep(Duration::from_millis(600 * 3u64.pow(attempt - 1))).await;
        }
    }

    pub async fn get_text(&self, url: &str) -> AppResult<String> {
        let resp = self.send(url, |c| c.get(url)).await?;
        check_status(&resp)?;
        Ok(resp.text().await?)
    }

    pub async fn get_json(&self, url: &str) -> AppResult<serde_json::Value> {
        let resp = self.send(url, |c| c.get(url)).await?;
        check_status(&resp)?;
        Ok(resp.json().await?)
    }
}

pub fn check_status(resp: &Response) -> AppResult<()> {
    let s = resp.status();
    if s.is_success() {
        return Ok(());
    }
    let code = match s.as_u16() {
        401 | 403 => "forbidden",
        404 | 410 => "not_found",
        429 => "rate_limited",
        _ => "service_error",
    };
    Err(AppError::new(code, format!("HTTP {s} from {}", resp.url().host_str().unwrap_or("?"))))
}
