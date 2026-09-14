use std::path::{Path, PathBuf};

use crate::error::AppError;

pub const MAX_SOURCE_BYTES: usize = 20 * 1024 * 1024;
const BLOCKED_EXT: &[&str] = &[
    "exe", "bat", "cmd", "com", "dll", "msi", "scr", "ps1", "vbs",
];

pub fn check(filename: &str, bytes: &[u8]) -> Result<String, AppError> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(AppError::bad("файл больше 20 МБ"));
    }
    let ext = extension_of(filename);
    if BLOCKED_EXT.contains(&ext.as_str()) {
        return Err(AppError::bad("этот тип файла нельзя импортировать"));
    }
    Ok(ext)
}

pub async fn save(
    data_dir: &Path,
    folder: &str,
    id: &str,
    filename: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<(String, String, String), AppError> {
    let ext = check(filename, bytes)?;
    let disk_name = if ext.is_empty() {
        "original".to_string()
    } else {
        format!("original.{ext}")
    };
    let rel = format!("{folder}/{id}/{disk_name}");
    let abs = data_dir.join(&rel);
    if let Some(parent) = abs.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&abs, bytes).await?;
    let mime = if mime.trim().is_empty() {
        "application/octet-stream".to_string()
    } else {
        mime.trim().to_string()
    };
    Ok((display_name(filename), mime, rel))
}

pub fn remove(data_dir: &Path, rel: &str) {
    if !safe_rel(rel) {
        return;
    }
    let abs: PathBuf = data_dir.join(rel);
    let _ = std::fs::remove_file(&abs);
    if let Some(parent) = abs.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

pub async fn read(data_dir: &Path, rel: &str) -> Result<Vec<u8>, AppError> {
    if !safe_rel(rel) {
        return Err(AppError::NotFound);
    }
    Ok(tokio::fs::read(data_dir.join(rel)).await?)
}

pub fn safe_rel(rel: &str) -> bool {
    !rel.is_empty() && !Path::new(rel).is_absolute() && !rel.contains("..")
}

fn extension_of(filename: &str) -> String {
    Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn display_name(filename: &str) -> String {
    Path::new(filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .chars()
        .take(180)
        .collect()
}
