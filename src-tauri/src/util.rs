use argon2::password_hash::rand_core::{OsRng, RngCore};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// A name usable as a Windows file name (without extension): reserved characters replaced,
/// trailing dots and spaces removed, at most 120 characters.
pub fn safe_file_stem(name: &str, fallback: &str) -> String {
    let clean: String = name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let clean: String = clean.trim().trim_end_matches(['.', ' ']).chars().take(120).collect();
    if clean.trim().is_empty() {
        fallback.to_string()
    } else {
        clean.trim_end().to_string()
    }
}

pub fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// RFC 4122 v4 UUID. Every syncable row gets one so records can be merged across PCs later.
pub fn uuid_v4() -> String {
    let mut b = random_bytes::<16>();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// Reads a query parameter from a URL without pulling in a URL parser.
pub fn query_param<'a>(url: &'a str, name: &str) -> Option<&'a str> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name).then_some(v)
    })
}

/// Percent-encodes a string for use in a query parameter.
pub fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A file dialog owned by the main window, so it opens over Relay and can't end up behind it.
pub fn file_dialog(app: &tauri::AppHandle) -> tauri_plugin_dialog::FileDialogBuilder<tauri::Wry> {
    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;
    let dialog = app.dialog().file();
    match app.get_webview_window("main") {
        Some(window) => dialog.set_parent(&window),
        None => dialog,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_has_v4_shape() {
        let id = uuid_v4();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
    }

    #[test]
    fn reads_query_params() {
        let url = "https://x.test/a?expire=1700000000&id=5";
        assert_eq!(query_param(url, "expire"), Some("1700000000"));
        assert_eq!(query_param(url, "id"), Some("5"));
        assert_eq!(query_param(url, "nope"), None);
    }

    #[test]
    fn encodes_query() {
        assert_eq!(encode_query("a b&c"), "a%20b%26c");
        assert_eq!(encode_query("ы"), "%D1%8B");
    }
}
