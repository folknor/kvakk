use std::collections::HashMap;
use std::path::Path;

use reqwest::{Body, Client as HttpClient, StatusCode};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use super::error::{LocalSendError, Result};
use super::protocol::{
    DeviceInfo, FileId, FileMetadata, PrepareUploadRequest, PrepareUploadResponse, SessionId,
    Token,
};

#[derive(Clone)]
pub struct LocalSendClient {
    client: HttpClient,
    device: DeviceInfo,
}

impl LocalSendClient {
    pub fn new(device: DeviceInfo) -> Self {
        // `danger_accept_invalid_certs` is required for the HTTPS peers that
        // use self-signed certs (standard LocalSend setup).
        let client = HttpClient::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_else(|_| HttpClient::new());

        Self { client, device }
    }

    fn target_ip<'a>(&self, target: &'a DeviceInfo) -> Result<&'a str> {
        target
            .ip
            .as_deref()
            .ok_or_else(|| LocalSendError::network("Target IP not provided"))
    }

    pub async fn register(&self, target: &DeviceInfo) -> Result<DeviceInfo> {
        let ip = self.target_ip(target)?;
        let url = format!(
            "{}://{}:{}/api/localsend/v2/register",
            target.protocol, ip, target.port
        );

        let response = self.client.post(&url).json(&self.device).send().await?;
        let status = response.status();

        if status.is_success() {
            let bytes = response.bytes().await?;
            if bytes.is_empty() {
                return Ok(target.clone());
            }
            // Some peers respond with a slightly different JSON shape; the
            // registration still succeeded from their POV, so don't fail here.
            Ok(serde_json::from_slice::<DeviceInfo>(&bytes).unwrap_or_else(|_| target.clone()))
        } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            Err(LocalSendError::Rejected { status: status.as_u16() })
        } else {
            Err(LocalSendError::http_failed(status.as_u16(), "Registration failed"))
        }
    }

    pub async fn prepare_upload(
        &self,
        target: &DeviceInfo,
        files: HashMap<FileId, FileMetadata>,
    ) -> Result<PrepareUploadResponse> {
        let ip = self.target_ip(target)?;
        let url = format!(
            "{}://{}:{}/api/localsend/v2/prepare-upload",
            target.protocol, ip, target.port
        );

        let request = PrepareUploadRequest { info: self.device.clone(), files };
        let response = self.client.post(&url).json(&request).send().await?;
        let status = response.status();

        match status {
            StatusCode::OK => Ok(response.json().await?),
            // 204 is returned for text-only transfers; signal completion via empty session id.
            StatusCode::NO_CONTENT => Ok(PrepareUploadResponse {
                session_id: SessionId::from_string(String::new()),
                files: HashMap::new(),
            }),
            StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => {
                Err(LocalSendError::Rejected { status: status.as_u16() })
            }
            StatusCode::CONFLICT => Err(LocalSendError::SessionBlocked),
            StatusCode::TOO_MANY_REQUESTS => Err(LocalSendError::RateLimited),
            StatusCode::INTERNAL_SERVER_ERROR => Err(LocalSendError::network("Server error")),
            _ => Err(LocalSendError::http_failed(status.as_u16(), "Prepare upload failed")),
        }
    }

    pub async fn upload_file(
        &self,
        target: &DeviceInfo,
        session_id: &SessionId,
        file_id: &FileId,
        token: &Token,
        file_path: &Path,
    ) -> Result<()> {
        let ip = self.target_ip(target)?;
        let url = format!(
            "{}://{}:{}/api/localsend/v2/upload?sessionId={}&fileId={}&token={}",
            target.protocol, ip, target.port, session_id, file_id, token
        );

        let file = File::open(file_path).await?;
        let body = Body::wrap_stream(ReaderStream::new(file));
        let response = self.client.post(&url).body(body).send().await?;
        let status = response.status();

        match status {
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
            _ => Err(LocalSendError::http_failed(status.as_u16(), "File upload failed")),
        }
    }
}
