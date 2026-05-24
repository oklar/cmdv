use std::sync::OnceLock;

use reqwest::Client;
use serde::Deserialize;
use zeroize::Zeroize;

use super::api_base_url;

static HTTP: OnceLock<Client> = OnceLock::new();

fn http() -> &'static Client {
    HTTP.get_or_init(|| {
        let mut builder = Client::builder()
            .cookie_store(true)
            .timeout(std::time::Duration::from_secs(30));
        // Local dev uses https://localhost with the ASP.NET dev certificate.
        if cfg!(debug_assertions) {
            builder = builder.danger_accept_invalid_certs(true);
        }
        builder.build().expect("reqwest client")
    })
}

#[derive(Deserialize)]
struct CsrfResponse {
    #[serde(rename = "requestToken")]
    request_token: String,
}

#[derive(Deserialize)]
struct AddClipboardResponse {
    url: String,
}

pub(crate) fn clipboard_request_body(data_b64: &str) -> serde_json::Value {
    serde_json::json!({ "data": data_b64 })
}

pub async fn upload_ciphertext(data_b64: &str) -> Result<String, String> {
    let base = api_base_url();
    let mut csrf = fetch_csrf_token(base).await?;

    let res = http()
        .post(format!("{base}/clipboard"))
        .header("Content-Type", "application/json")
        .header("X-XSRF-TOKEN", &csrf)
        .json(&clipboard_request_body(data_b64))
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    csrf.zeroize();

    if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("Rate limited — try again in a minute.".into());
    }

    if !res.status().is_success() {
        let status = res.status();
        log::warn!("POST /clipboard failed: {status}");
        return Err(format!("Server returned {status}"));
    }

    let parsed: AddClipboardResponse = res.json().await.map_err(|e| e.to_string())?;
    Ok(parsed.url)
}

async fn fetch_csrf_token(base: &str) -> Result<String, String> {
    let res = http()
        .get(format!("{base}/antiforgery/token"))
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("Could not fetch CSRF token ({})", res.status()));
    }

    let parsed: CsrfResponse = res.json().await.map_err(|e| e.to_string())?;
    Ok(parsed.request_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_request_body_matches_api_contract() {
        let body = clipboard_request_body("abc123+/=");
        assert_eq!(body["data"].as_str(), Some("abc123+/="));
    }
}
