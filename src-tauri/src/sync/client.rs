//! HTTP calls to the sync API. See "Cloud sync" in the backend README for the endpoints.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::sync::item::{ItemKind, SyncItem};

pub const DAILY_QUOTA_TYPE: &str = "urn:cmdv:sync:daily-quota";
pub const PINNED_LIMIT_TYPE: &str = "urn:cmdv:sync:pinned-limit";
pub const STALE_KEY_VERSION_TYPE: &str = "urn:cmdv:sync:stale-key-version";

pub type TokenFuture<'a> = Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

/// Supplies the account's access token. `refresh` is true after the API refused the last one.
pub trait AccessTokenSource: Send + Sync {
    fn access_token(&self, refresh: bool) -> TokenFuture<'_>;
}

#[derive(Debug)]
pub enum ApiError {
    Network(String),
    Auth(String),
    Status {
        status: u16,
        problem_type: Option<String>,
    },
    Invalid(String),
}

pub struct KeyState {
    pub generation_id: String,
    pub key_version: u32,
    pub bundle: Option<Vec<u8>>,
    pub latest_seq: i64,
}

pub struct SavedItems {
    pub generation_id: String,
    pub seqs: Vec<i64>,
    pub latest_seq: i64,
}

pub struct Change {
    pub seq: i64,
    pub item: SyncItem,
}

pub struct ChangesPage {
    pub generation_id: String,
    pub items: Vec<Change>,
    pub latest_seq: i64,
    pub has_more: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyBody {
    generation_id: String,
    key_version: u32,
    bundle: Option<String>,
    latest_seq: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ItemBody<'a> {
    id: &'a str,
    kind: &'static str,
    key_version: u32,
    pinned: bool,
    ciphertext: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedBody {
    generation_id: String,
    seqs: Vec<i64>,
    latest_seq: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangeBody {
    id: String,
    seq: i64,
    kind: String,
    pinned: bool,
    ciphertext: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangesBody {
    generation_id: String,
    key_version: u32,
    items: Vec<ChangeBody>,
    latest_seq: i64,
    has_more: bool,
}

#[derive(Deserialize)]
struct ProblemBody {
    #[serde(rename = "type")]
    problem_type: Option<String>,
}

pub struct SyncClient {
    base_url: String,
    tokens: Arc<dyn AccessTokenSource>,
}

impl SyncClient {
    pub fn new(base_url: impl Into<String>, tokens: Arc<dyn AccessTokenSource>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            tokens,
        }
    }

    pub async fn get_key(&self) -> Result<KeyState, ApiError> {
        let url = format!("{}/sync/key", self.base_url);
        let res = self.send(|http| http.get(&url)).await?;
        key_state(expect_success(res).await?).await
    }

    /// Stores the account's first key bundle. If another device stored one first, returns that.
    pub async fn put_key(&self, sealed: &[u8]) -> Result<KeyState, ApiError> {
        let url = format!("{}/sync/key", self.base_url);
        let body = serde_json::json!({ "bundle": STANDARD.encode(sealed) });
        let res = self.send(|http| http.put(&url).json(&body)).await?;
        if res.status() == StatusCode::CONFLICT {
            return key_state(res).await;
        }
        key_state(expect_success(res).await?).await
    }

    pub async fn post_items(&self, items: &[SyncItem]) -> Result<SavedItems, ApiError> {
        let url = format!("{}/sync/items", self.base_url);
        let items: Vec<ItemBody> = items
            .iter()
            .map(|item| ItemBody {
                id: &item.id,
                kind: item.kind.as_str(),
                key_version: item.key_version,
                pinned: item.pinned,
                ciphertext: STANDARD.encode(&item.ciphertext),
            })
            .collect();
        let body = serde_json::to_vec(&serde_json::json!({ "items": items }))
            .map_err(|e| ApiError::Invalid(e.to_string()))?;

        let res = self
            .send(|http| {
                http.post(&url)
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(body.clone())
            })
            .await?;
        let saved: SavedBody = json(expect_success(res).await?).await?;
        Ok(SavedItems {
            generation_id: saved.generation_id,
            seqs: saved.seqs,
            latest_seq: saved.latest_seq,
        })
    }

    pub async fn get_changes(&self, after: i64, limit: u32) -> Result<ChangesPage, ApiError> {
        let url = format!("{}/sync/changes?after={after}&limit={limit}", self.base_url);
        let res = self.send(|http| http.get(&url)).await?;
        let page: ChangesBody = json(expect_success(res).await?).await?;

        let items = page
            .items
            .into_iter()
            .map(|change| {
                Ok(Change {
                    seq: change.seq,
                    item: SyncItem {
                        kind: ItemKind::parse(&change.kind).ok_or_else(|| {
                            ApiError::Invalid(format!("unknown kind {}", change.kind))
                        })?,
                        id: change.id,
                        key_version: page.key_version,
                        pinned: change.pinned,
                        ciphertext: STANDARD
                            .decode(change.ciphertext)
                            .map_err(|e| ApiError::Invalid(e.to_string()))?,
                    },
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;

        Ok(ChangesPage {
            generation_id: page.generation_id,
            items,
            latest_seq: page.latest_seq,
            has_more: page.has_more,
        })
    }

    /// Sends with the current access token, and once more with a fresh one after a 401.
    async fn send(
        &self,
        request: impl Fn(&Client) -> RequestBuilder,
    ) -> Result<Response, ApiError> {
        let http = crate::commands::account_auth::http();
        let mut token = self
            .tokens
            .access_token(false)
            .await
            .map_err(ApiError::Auth)?;
        let mut res = request(http).bearer_auth(&token).send().await;
        token.zeroize();

        if matches!(&res, Ok(r) if r.status() == StatusCode::UNAUTHORIZED) {
            let mut token = self
                .tokens
                .access_token(true)
                .await
                .map_err(ApiError::Auth)?;
            res = request(http).bearer_auth(&token).send().await;
            token.zeroize();
        }
        res.map_err(|e| ApiError::Network(e.to_string()))
    }
}

async fn expect_success(res: Response) -> Result<Response, ApiError> {
    if res.status().is_success() {
        return Ok(res);
    }
    let status = res.status().as_u16();
    let problem_type = res
        .json::<ProblemBody>()
        .await
        .ok()
        .and_then(|problem| problem.problem_type);
    Err(ApiError::Status {
        status,
        problem_type,
    })
}

async fn json<T: serde::de::DeserializeOwned>(res: Response) -> Result<T, ApiError> {
    res.json()
        .await
        .map_err(|e| ApiError::Invalid(e.to_string()))
}

async fn key_state(res: Response) -> Result<KeyState, ApiError> {
    let body: KeyBody = json(res).await?;
    Ok(KeyState {
        generation_id: body.generation_id,
        key_version: body.key_version,
        bundle: body
            .bundle
            .map(|b| STANDARD.decode(b))
            .transpose()
            .map_err(|e| ApiError::Invalid(e.to_string()))?,
        latest_seq: body.latest_seq,
    })
}
