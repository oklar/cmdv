//! Reveals the recovery phrase. The words always come from the keychain, never from the webview.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use tauri::{State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use zeroize::{Zeroize, Zeroizing};

use crate::crypto::keys::{MasterKey, VaultState};
use crate::os_auth::{self, OsAuth};
use crate::storage::keychain::KeychainStore;
use crate::storage::SeedStore;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

const OS_AUTH_MESSAGE: &str = "Confirm it's you to show your cmdv recovery phrase";

fn ensure_unlocked(vault: &VaultState) -> Result<(), String> {
    if vault.keys.lock().map_err(|_| "Lock poisoned")?.is_none() {
        return Err("Vault is locked".into());
    }
    Ok(())
}

fn ensure_setup_in_progress(vault: &VaultState) -> Result<(), String> {
    if vault.setup_complete.load(Ordering::Relaxed) {
        return Err("The recovery phrase can only be saved during setup.".into());
    }
    Ok(())
}

fn recovery_words(vault: &VaultState) -> Result<Zeroizing<Vec<String>>, String> {
    ensure_unlocked(vault)?;
    let mut seed = KeychainStore::new().load_seed()?;
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&seed[..32]);
    seed.zeroize();
    let master_key = MasterKey::from_bytes(bytes);
    bytes.zeroize();

    let mnemonic = bip39::Mnemonic::from_entropy(master_key.as_bytes())
        .map_err(|e| format!("BIP39 error: {e}"))?;
    Ok(Zeroizing::new(mnemonic.words().map(String::from).collect()))
}

/// Returns false if the user cancels the save dialog.
#[tauri::command]
pub async fn save_recovery_phrase(
    window: WebviewWindow,
    vault: State<'_, Arc<VaultState>>,
    format: String,
) -> Result<bool, String> {
    ensure_setup_in_progress(&vault)?;
    ensure_unlocked(&vault)?;
    let format = SaveFormat::parse(&format)?;

    let dialog_window = window.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_window
            .dialog()
            .file()
            .set_parent(&dialog_window)
            .set_file_name(format.file_name())
            .add_filter(format.filter_name(), &[format.extension()])
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;

    let Some(picked) = picked else {
        return Ok(false);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;

    let words = recovery_words(&vault)?;
    match format {
        SaveFormat::Pdf => write_pdf(&path, &words)?,
        SaveFormat::Text => {
            let mut text = generate_txt(&words);
            let result = std::fs::write(&path, text.as_bytes());
            text.zeroize();
            result.map_err(|e| format!("Failed to write file: {e}"))?;
        }
    }
    Ok(true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveFormat {
    Pdf,
    Text,
}

impl SaveFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "pdf" => Ok(Self::Pdf),
            "txt" => Ok(Self::Text),
            other => Err(format!("Unknown format: {other}")),
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Text => "txt",
        }
    }

    fn filter_name(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Text => "Text",
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Self::Pdf => "cmdv-recovery-kit.pdf",
            Self::Text => "cmdv-recovery-phrase.txt",
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RecoveryQr {
    Shown { qr: String },
    AuthUnavailable,
    Cancelled,
}

/// `allow_without_os_auth` only matters on devices without Windows Hello.
#[tauri::command]
pub async fn show_recovery_qr(
    window: WebviewWindow,
    vault: State<'_, Arc<VaultState>>,
    allow_without_os_auth: bool,
) -> Result<RecoveryQr, String> {
    ensure_unlocked(&vault)?;

    let native_window = native_window_handle(&window);
    crate::OS_AUTH_PROMPT_OPEN.store(true, Ordering::Relaxed);
    let auth = tauri::async_runtime::spawn_blocking(move || {
        os_auth::verify_user(native_window, OS_AUTH_MESSAGE)
    })
    .await;
    crate::OS_AUTH_PROMPT_OPEN.store(false, Ordering::Relaxed);
    let _ = window.set_focus();

    match auth.map_err(|e| e.to_string())?? {
        OsAuth::Verified => {}
        OsAuth::Cancelled => return Ok(RecoveryQr::Cancelled),
        OsAuth::Unavailable if allow_without_os_auth => {}
        OsAuth::Unavailable => return Ok(RecoveryQr::AuthUnavailable),
    }

    let words = recovery_words(&vault)?;
    Ok(RecoveryQr::Shown {
        qr: qr_data_url(&words)?,
    })
}

#[cfg(windows)]
fn native_window_handle(window: &WebviewWindow) -> isize {
    window.hwnd().map(|hwnd| hwnd.0 as isize).unwrap_or(0)
}

#[cfg(not(windows))]
fn native_window_handle(_window: &WebviewWindow) -> isize {
    0
}

fn qr_data_url(words: &[String]) -> Result<String, String> {
    use qrcode::render::svg;
    use qrcode::QrCode;

    let mut payload = words.join(" ");
    let code = QrCode::new(payload.as_bytes());
    payload.zeroize();
    let code = code.map_err(|e| format!("QR generation error: {e}"))?;
    let svg_str = code.render::<svg::Color>().min_dimensions(256, 256).build();

    Ok(format!(
        "data:image/svg+xml;base64,{}",
        base64::Engine::encode(&B64, svg_str.as_bytes())
    ))
}

fn generate_txt(words: &[String]) -> String {
    let mut out = String::new();
    out.push_str("CMDV Recovery Phrase\n");
    out.push_str("===================\n\n");
    out.push_str("Keep this file in a safe place. These 24 words are the key\n");
    out.push_str("to your data — you need them to restore CMDV on a new device.\n\n");
    for (i, word) in words.iter().enumerate() {
        out.push_str(&format!("{:>2}. {}\n", i + 1, word));
    }
    out.push_str("\nWARNING: Anyone with these words can decrypt your clipboard\n");
    out.push_str("data. Delete this file after storing the phrase securely.\n");
    out
}

fn write_pdf(path: &Path, words: &[String]) -> Result<(), String> {
    let mut lines: Vec<String> = Vec::new();
    lines.push("CMDV Recovery Kit".into());
    lines.push(String::new());
    lines.push("Keep this document in a safe place. These 24 words are the key".into());
    lines.push("to your data — you need them to restore CMDV on a new device.".into());
    lines.push(String::new());

    for (chunk_idx, chunk) in words.chunks(4).enumerate() {
        let idx_start = chunk_idx * 4;
        let row: String = chunk
            .iter()
            .enumerate()
            .map(|(j, w)| format!("{:>2}. {:<14}", idx_start + j + 1, w))
            .collect::<Vec<_>>()
            .join("  ");
        lines.push(row);
    }

    lines.push(String::new());
    lines.push("WARNING: Anyone with these words can decrypt your clipboard".into());
    lines.push("data. Delete this file after storing the phrase securely.".into());

    let font_size = 11;
    let title_size = 18;
    let leading = 16;
    let page_width = 595;
    let page_height = 842;
    let margin = 50;

    let mut text_ops = String::new();
    text_ops.push_str("BT\n");

    for (i, line) in lines.iter().enumerate() {
        let y = page_height - margin - (i as i32) * leading;
        if y < margin {
            break;
        }
        let size = if i == 0 { title_size } else { font_size };
        let escaped = line
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        if i == 0 {
            text_ops.push_str(&format!(
                "/F1 {} Tf {} {} Td ({}) Tj\n",
                size, margin, y, escaped
            ));
        } else {
            text_ops.push_str(&format!(
                "/F1 {} Tf 0 -{} Td ({}) Tj\n",
                size, leading, escaped
            ));
        }
    }
    text_ops.push_str("ET\n");
    lines.zeroize();

    let stream = text_ops;
    let stream_len = stream.len();

    let mut pdf = String::new();
    let mut offsets: Vec<usize> = Vec::new();

    pdf.push_str("%PDF-1.4\n");

    offsets.push(pdf.len());
    pdf.push_str("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.push_str("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    offsets.push(pdf.len());
    pdf.push_str(&format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n",
        page_width, page_height
    ));

    offsets.push(pdf.len());
    pdf.push_str(&format!(
        "4 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
        stream_len, stream
    ));

    offsets.push(pdf.len());
    pdf.push_str("5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>\nendobj\n");

    let xref_offset = pdf.len();
    pdf.push_str("xref\n");
    pdf.push_str(&format!("0 {}\n", offsets.len() + 1));
    pdf.push_str("0000000000 65535 f \n");
    for off in &offsets {
        pdf.push_str(&format!("{:010} 00000 n \n", off));
    }

    pdf.push_str("trailer\n");
    pdf.push_str(&format!("<< /Size {} /Root 1 0 R >>\n", offsets.len() + 1));
    pdf.push_str("startxref\n");
    pdf.push_str(&format!("{}\n", xref_offset));
    pdf.push_str("%%EOF\n");

    let result = std::fs::write(path, pdf.as_bytes());
    let mut stream = stream;
    stream.zeroize();
    pdf.zeroize();
    result.map_err(|e| format!("Failed to write PDF: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_words() -> Vec<String> {
        (1..=24).map(|i| format!("word{i}")).collect()
    }

    #[test]
    fn save_is_refused_after_setup() {
        let vault = VaultState::new();
        assert!(ensure_setup_in_progress(&vault).is_ok());
        vault.setup_complete.store(true, Ordering::Relaxed);
        assert!(ensure_setup_in_progress(&vault).is_err());
    }

    #[test]
    fn recovery_words_need_an_unlocked_vault() {
        let vault = VaultState::new();
        assert_eq!(recovery_words(&vault).unwrap_err(), "Vault is locked");
    }

    #[test]
    fn save_format_accepts_only_pdf_and_txt() {
        assert_eq!(SaveFormat::parse("pdf"), Ok(SaveFormat::Pdf));
        assert_eq!(SaveFormat::parse("txt"), Ok(SaveFormat::Text));
        assert!(SaveFormat::parse("../../evil").is_err());
        assert_eq!(SaveFormat::Pdf.file_name(), "cmdv-recovery-kit.pdf");
        assert_eq!(SaveFormat::Text.extension(), "txt");
    }

    #[test]
    fn qr_result_serializes_with_a_status_tag() {
        let shown = serde_json::to_value(RecoveryQr::Shown { qr: "data:x".into() }).unwrap();
        assert_eq!(shown, serde_json::json!({ "status": "shown", "qr": "data:x" }));
        assert_eq!(
            serde_json::to_value(RecoveryQr::AuthUnavailable).unwrap(),
            serde_json::json!({ "status": "auth_unavailable" })
        );
        assert_eq!(
            serde_json::to_value(RecoveryQr::Cancelled).unwrap(),
            serde_json::json!({ "status": "cancelled" })
        );
    }

    #[test]
    fn qr_is_an_svg_data_url() {
        let url = qr_data_url(&sample_words()).unwrap();
        assert!(url.starts_with("data:image/svg+xml;base64,"));
    }

    #[test]
    fn text_export_lists_all_words_numbered() {
        let text = generate_txt(&sample_words());
        assert!(text.contains(" 1. word1\n"));
        assert!(text.contains("24. word24\n"));
    }

    #[test]
    fn pdf_export_writes_a_pdf_with_the_words() {
        let dir = std::env::temp_dir().join(format!("cmdv-recovery-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("kit.pdf");
        write_pdf(&path, &sample_words()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(String::from_utf8_lossy(&bytes).contains("word24"));
    }
}
