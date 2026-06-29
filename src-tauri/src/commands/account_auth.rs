use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_oauth::{start_with_config, OauthConfig};
use tauri_plugin_opener::OpenerExt;
use url::Url;
use zeroize::Zeroize;

use crate::commands::secure_paste::{api_base_url, paste_site_url};
use crate::storage::keychain::KeychainStore;

/// In-memory desktop auth session. The refresh token is persisted in the OS keychain; only
/// the short-lived access token plus display email live here.
#[derive(Default)]
pub struct AuthState {
    session: Mutex<Option<ActiveSession>>,
}

struct ActiveSession {
    access_token: String,
    email: String,
    expires_at: Instant,
}

impl AuthState {
    pub fn new() -> Self {
        Self::default()
    }

    fn set(&self, access_token: String, email: String, expires_in_secs: u64) {
        // Refresh a little early so an in-flight request never races expiry.
        let lifetime = Duration::from_secs(expires_in_secs.saturating_sub(30));
        let session = ActiveSession {
            access_token,
            email,
            expires_at: Instant::now() + lifetime,
        };
        if let Ok(mut guard) = self.session.lock() {
            if let Some(mut old) = guard.replace(session) {
                old.access_token.zeroize();
            }
        }
    }

    fn clear(&self) {
        if let Ok(mut guard) = self.session.lock() {
            if let Some(mut old) = guard.take() {
                old.access_token.zeroize();
            }
        }
    }

    fn valid_access_token(&self) -> Option<String> {
        let guard = self.session.lock().ok()?;
        let session = guard.as_ref()?;
        if Instant::now() < session.expires_at {
            Some(session.access_token.clone())
        } else {
            None
        }
    }

    fn email(&self) -> Option<String> {
        let guard = self.session.lock().ok()?;
        guard.as_ref().map(|s| s.email.clone())
    }
}

#[derive(Serialize, Clone)]
pub struct AccountStatus {
    pub logged_in: bool,
    pub email: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct AccountInfo {
    pub email: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "expiresIn")]
    expires_in: u64,
    email: String,
}

static HTTP: OnceLock<Client> = OnceLock::new();

fn http() -> &'static Client {
    HTTP.get_or_init(|| {
        let mut builder = Client::builder().timeout(Duration::from_secs(30));
        // Local dev uses https://localhost with the ASP.NET dev certificate.
        if cfg!(debug_assertions) {
            builder = builder.danger_accept_invalid_certs(true);
        }
        builder.build().expect("reqwest client")
    })
}

fn random_b64url(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    let encoded = URL_SAFE_NO_PAD.encode(&bytes);
    bytes.zeroize();
    encoded
}

fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn mask_email(email: &str) -> String {
    match email.find('@') {
        Some(at) if at >= 2 => format!("{}***{}", &email[..1], &email[at..]),
        Some(at) => format!("***@{}", &email[at + 1..]),
        None => "***@***".to_string(),
    }
}

/// Start the loopback + PKCE desktop login. Opens the web login in the system browser and
/// listens on a temporary localhost port for the authorization-code redirect.
#[tauri::command]
pub fn begin_desktop_login(app: AppHandle) -> Result<(), String> {
    let mut verifier = random_b64url(32);
    let challenge = code_challenge(&verifier);
    let state = random_b64url(16);

    let app_for_cb = app.clone();
    let state_for_cb = state.clone();
    let verifier_for_cb = verifier.clone();

    let config = OauthConfig {
        ports: None,
        response: Some("cmdv login complete. You can close this window and return to the app.".into()),
    };

    let port = start_with_config(config, move |callback_url| {
        let app = app_for_cb.clone();
        let expected_state = state_for_cb.clone();
        let mut verifier = verifier_for_cb.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = complete_login(&app, &callback_url, &expected_state, &verifier).await {
                log::warn!("Desktop login failed: {e}");
                let _ = app.emit("desktop-auth-error", e);
            }
            verifier.zeroize();
        });
    })
    .map_err(|e| format!("Could not start login listener: {e}"))?;

    // redirect_uri is fixed-shape, so encode it inline rather than pulling in a query builder.
    // state/challenge are base64url (already URL-safe), so they need no encoding.
    let redirect_param = format!("http%3A%2F%2F127.0.0.1%3A{port}");
    let login_url = format!(
        "{}/login?desktop=1&redirect_uri={}&state={}&code_challenge={}",
        paste_site_url().trim_end_matches('/'),
        redirect_param,
        state,
        challenge,
    );

    verifier.zeroize();

    app.opener()
        .open_url(login_url, None::<&str>)
        .map_err(|e| format!("Could not open browser: {e}"))?;

    Ok(())
}

async fn complete_login(
    app: &AppHandle,
    callback_url: &str,
    expected_state: &str,
    verifier: &str,
) -> Result<(), String> {
    let parsed = Url::parse(callback_url).map_err(|e| format!("Invalid callback URL: {e}"))?;

    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            _ => {}
        }
    }

    let mut code = code.ok_or("Missing authorization code in callback.")?;
    let state = state.ok_or("Missing state in callback.")?;

    if state.as_bytes().ct_eq(expected_state.as_bytes()).unwrap_u8() != 1 {
        code.zeroize();
        return Err("State mismatch — login was not completed safely.".into());
    }

    let res = http()
        .post(format!("{}/auth/desktop/token", api_base_url()))
        .json(&serde_json::json!({ "code": code, "codeVerifier": verifier }))
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    code.zeroize();

    if !res.status().is_success() {
        return Err(format!("Login failed (server returned {}).", res.status()));
    }

    let mut body: TokenResponse = res.json().await.map_err(|e| e.to_string())?;

    let masked_email = mask_email(&body.email);
    let store = KeychainStore::new();
    store.save_account_session(&body.refresh_token, &masked_email)?;

    app.state::<AuthState>()
        .set(body.access_token.clone(), masked_email.clone(), body.expires_in);

    body.access_token.zeroize();
    body.refresh_token.zeroize();

    app.emit("desktop-auth-success", AccountInfo { email: masked_email })
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn get_account_status(app: AppHandle) -> Result<AccountStatus, String> {
    if let Some(email) = app.state::<AuthState>().email() {
        return Ok(AccountStatus {
            logged_in: true,
            email: Some(email),
        });
    }

    let store = KeychainStore::new();
    let logged_in = store.load_account_refresh_token()?.is_some();
    let email = if logged_in {
        store.load_account_email()?
    } else {
        None
    };

    Ok(AccountStatus { logged_in, email })
}

#[tauri::command]
pub async fn fetch_account(app: AppHandle) -> Result<AccountInfo, String> {
    let mut token = ensure_access_token(&app).await?;
    let mut authorized = get_me_ok(&token).await?;

    if !authorized {
        token.zeroize();
        token = refresh_session(&app).await?;
        authorized = get_me_ok(&token).await?;
    }
    token.zeroize();

    if !authorized {
        let store = KeychainStore::new();
        store.delete_account_session()?;
        app.state::<AuthState>().clear();
        return Err("Session expired. Please log in again.".into());
    }

    let email = app
        .state::<AuthState>()
        .email()
        .or_else(|| KeychainStore::new().load_account_email().ok().flatten())
        .ok_or("Missing account email.")?;

    Ok(AccountInfo { email })
}

#[tauri::command]
pub fn desktop_logout(app: AppHandle) -> Result<(), String> {
    KeychainStore::new().delete_account_session()?;
    app.state::<AuthState>().clear();
    Ok(())
}

async fn ensure_access_token(app: &AppHandle) -> Result<String, String> {
    if let Some(token) = app.state::<AuthState>().valid_access_token() {
        return Ok(token);
    }
    refresh_session(app).await
}

async fn refresh_session(app: &AppHandle) -> Result<String, String> {
    let store = KeychainStore::new();
    let mut refresh = store
        .load_account_refresh_token()?
        .ok_or("Not logged in.")?;

    let res = http()
        .post(format!("{}/auth/desktop/refresh", api_base_url()))
        .json(&serde_json::json!({ "refreshToken": refresh }))
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"));
    refresh.zeroize();
    let res = res?;

    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        store.delete_account_session()?;
        app.state::<AuthState>().clear();
        return Err("Session expired. Please log in again.".into());
    }
    if !res.status().is_success() {
        return Err(format!("Server returned {}", res.status()));
    }

    let mut body: TokenResponse = res.json().await.map_err(|e| e.to_string())?;
    let masked_email = mask_email(&body.email);
    store.save_account_session(&body.refresh_token, &masked_email)?;
    app.state::<AuthState>()
        .set(body.access_token.clone(), masked_email, body.expires_in);

    let token = body.access_token.clone();
    body.access_token.zeroize();
    body.refresh_token.zeroize();
    Ok(token)
}

async fn get_me_ok(access_token: &str) -> Result<bool, String> {
    let res = http()
        .get(format!("{}/auth/me", api_base_url()))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(false);
    }
    if !res.status().is_success() {
        return Err(format!("Server returned {}", res.status()));
    }
    Ok(true)
}
