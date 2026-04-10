use std::path::Path;

use mime_guess::from_path;

use super::error::Result;
use super::protocol::{DeviceType, FileId, FileMetadata};

pub fn get_device_model() -> String {
    std::env::consts::OS.to_string()
}

pub fn get_device_type() -> DeviceType {
    if cfg!(target_os = "android") || cfg!(target_os = "ios") {
        DeviceType::Mobile
    } else if cfg!(target_arch = "wasm32") {
        DeviceType::Web
    } else {
        DeviceType::Desktop
    }
}

/// Generate a unique fingerprint for device identification.
pub fn generate_fingerprint() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub async fn build_file_metadata(path: &Path) -> Result<FileMetadata> {
    let metadata = tokio::fs::metadata(path).await?;

    Ok(FileMetadata {
        id: FileId::new(),
        file_name: path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("unknown"))
            .to_string_lossy()
            .to_string(),
        size: metadata.len(),
        file_type: from_path(path).first_or_octet_stream().to_string(),
        sha256: None,
        preview: None,
        metadata: None,
    })
}
