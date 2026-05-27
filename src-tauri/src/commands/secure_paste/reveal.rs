use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use zeroize::Zeroize;

use crate::crypto::paste_encrypt;

use super::api;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteRevealPayload {
    pub plaintext: Option<String>,
    pub error: Option<String>,
}

/// Parse `cmdv://paste?id={pasteId}&key={encryptionKey}`.
pub fn parse_paste_deep_link(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    let rest = url.strip_prefix("cmdv://")?;
    let query = rest.split('?').nth(1)?;
    let mut paste_id = None;
    let mut key = None;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=')?;
        let value = percent_decode(value);
        match name {
            "id" if !value.is_empty() => paste_id = Some(value),
            "key" if !value.is_empty() => key = Some(value),
            _ => {}
        }
    }
    Some((paste_id?, key?))
}

fn percent_decode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &bytes[i + 1..i + 3];
            if let Ok(s) = std::str::from_utf8(hex) {
                if let Ok(byte) = u8::from_str_radix(s, 16) {
                    out.push(byte as char);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

pub async fn run_reveal_secure_paste(paste_id: &str, key_b64: &str) -> Result<String, String> {
    let mut data_b64 = api::fetch_ciphertext(paste_id).await?;
    let plaintext = paste_encrypt::decrypt_paste(&data_b64, key_b64)?;
    data_b64.zeroize();
    Ok(plaintext)
}

fn emit_reveal_result(app: &AppHandle, plaintext: Option<String>, error: Option<String>) {
    let _ = app.emit(
        "deep-link-paste",
        PasteRevealPayload { plaintext, error },
    );
}

pub fn handle_deep_link_urls(app: &AppHandle, urls: &[String]) {
    for url in urls {
        let Some((paste_id, mut key)) = parse_paste_deep_link(url) else {
            log::warn!("Ignoring unrecognized deep link");
            continue;
        };

        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }

        let _ = app.emit("deep-link-paste-loading", ());

        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let result = run_reveal_secure_paste(&paste_id, &key).await;
            key.zeroize();
            match result {
                Ok(plaintext) => emit_reveal_result(&app, Some(plaintext), None),
                Err(e) => {
                    log::warn!("Secure paste reveal failed: {e}");
                    emit_reveal_result(&app, None, Some(e));
                }
            }
        });
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_paste_deep_link() {
        let url = "cmdv://paste?id=abc123&key=YWJj";
        let (id, key) = parse_paste_deep_link(url).unwrap();
        assert_eq!(id, "abc123");
        assert_eq!(key, "YWJj");
    }

    #[test]
    fn parse_rejects_missing_key() {
        assert!(parse_paste_deep_link("cmdv://paste?id=only").is_none());
    }

    #[test]
    fn parse_rejects_non_cmdv_scheme() {
        assert!(parse_paste_deep_link("https://cmdv.to/paste").is_none());
    }
}
