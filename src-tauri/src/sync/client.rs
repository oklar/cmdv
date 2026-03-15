use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct SyncClient {
    http: Client,
    api_base: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedUrlResponse {
    pub url: String,
    pub etag: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncStatusResponse {
    pub syncs_remaining_today: i32,
    pub rollover_balance: i32,
    pub last_sync_at: Option<String>,
}

impl SyncClient {
    pub fn new(api_base: &str) -> Self {
        Self {
            http: Client::new(),
            api_base: api_base.to_string(),
        }
    }

    pub async fn get_download_url(&self, token: &str) -> Result<SignedUrlResponse, String> {
        let resp = self
            .http
            .get(format!("{}/sync/blob", self.api_base))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("Failed to get download URL: {}", resp.status()));
        }

        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn get_upload_url(&self, token: &str) -> Result<SignedUrlResponse, String> {
        let resp = self
            .http
            .get(format!("{}/sync/blob/upload", self.api_base))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("Failed to get upload URL: {}", resp.status()));
        }

        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn download_blob(
        &self,
        signed_url: &str,
    ) -> Result<(Vec<u8>, Option<String>), String> {
        let resp = self
            .http
            .get(signed_url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        Ok((bytes.to_vec(), etag))
    }

    pub async fn upload_blob(
        &self,
        signed_url: &str,
        data: Vec<u8>,
        etag: Option<&str>,
    ) -> Result<(), String> {
        let mut req = self.http.put(signed_url).body(data);

        if let Some(etag) = etag {
            req = req.header("If-Match", etag);
        }

        let resp = req.send().await.map_err(|e| e.to_string())?;

        if resp.status().as_u16() == 412 {
            return Err("Conflict: ETag mismatch".into());
        }
        if !resp.status().is_success() {
            return Err(format!("Upload failed: {}", resp.status()));
        }

        Ok(())
    }

    pub async fn get_sync_status(&self, token: &str) -> Result<SyncStatusResponse, String> {
        let resp = self
            .http
            .get(format!("{}/sync/status", self.api_base))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("Failed to get sync status: {}", resp.status()));
        }

        resp.json().await.map_err(|e| e.to_string())
    }
}
