//! Serves cover images through `relay://img/<url>` with CORS headers, so the webview can read
//! their pixels for the cover-color backdrop. A small in-memory cache keeps repeat loads instant.

use crate::AppState;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tauri::http::{header, Request, Response, StatusCode};

const MAX_BYTES: usize = 6 * 1024 * 1024;
const KEEP: usize = 48;

/// (url, image bytes, mime type)
type Entry = (String, Arc<Vec<u8>>, String);

#[derive(Default)]
pub struct Covers(Mutex<VecDeque<Entry>>);

impl Covers {
    fn get(&self, url: &str) -> Option<(Arc<Vec<u8>>, String)> {
        let list = self.0.lock().unwrap_or_else(|p| p.into_inner());
        list.iter().find(|(u, _, _)| u == url).map(|(_, b, m)| (b.clone(), m.clone()))
    }

    fn put(&self, url: String, bytes: Arc<Vec<u8>>, mime: String) {
        let mut list = self.0.lock().unwrap_or_else(|p| p.into_inner());
        list.push_back((url, bytes, mime));
        while list.len() > KEEP {
            list.pop_front();
        }
    }
}

fn respond(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "max-age=86400")
        .body(body)
        .expect("valid response")
}

pub async fn serve(state: Arc<AppState>, req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let raw = req.uri().path().trim_start_matches("/img/");
    let url = crate::streams::percent_decode(raw);
    if !url.starts_with("https://") || url.len() > 2048 {
        return respond(StatusCode::BAD_REQUEST, "text/plain", b"bad url".to_vec());
    }
    if let Some((bytes, mime)) = state.covers.get(&url) {
        return respond(StatusCode::OK, &mime, bytes.to_vec());
    }
    let fetched = async {
        let resp = state.http.send(&url, |c| c.get(&url)).await.ok()?;
        crate::http::check_status(&resp).ok()?;
        let mime = resp.headers().get(header::CONTENT_TYPE)?.to_str().ok()?.to_string();
        if !mime.starts_with("image/") || resp.content_length().is_some_and(|l| l as usize > MAX_BYTES) {
            return None;
        }
        let bytes = resp.bytes().await.ok()?;
        (bytes.len() <= MAX_BYTES).then(|| (bytes.to_vec(), mime))
    };
    match fetched.await {
        Some((bytes, mime)) => {
            let bytes = Arc::new(bytes);
            state.covers.put(url, bytes.clone(), mime.clone());
            respond(StatusCode::OK, &mime, bytes.to_vec())
        }
        None => respond(StatusCode::BAD_GATEWAY, "text/plain", b"cover unavailable".to_vec()),
    }
}
