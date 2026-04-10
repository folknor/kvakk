use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::{RwLock, oneshot};
use tokio::task::JoinHandle;

use super::device::{generate_fingerprint, get_device_model, get_device_type};
use super::error::Result;
use super::protocol::{
    DeviceInfo, FileId, FileMetadata, PROTOCOL_VERSION, PrepareUploadRequest,
    PrepareUploadResponse, Protocol, ReceivedFile, SessionId, Token,
};

/// How long to wait for user acceptance of a new transfer.
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(60);
/// After this long, an idle session is discarded.
const SESSION_IDLE_LIMIT: Duration = Duration::from_secs(300);

/// A transfer waiting for user acceptance. Drop `response_tx` to reject;
/// send `true` to accept.
pub struct PendingTransfer {
    pub sender: DeviceInfo,
    pub files: HashMap<FileId, FileMetadata>,
    pub response_tx: oneshot::Sender<bool>,
}

struct ActiveSession {
    session_id: SessionId,
    files: HashMap<FileId, FileMetadata>,
    sender_alias: String,
    last_activity: Instant,
}

struct ServerState {
    device: DeviceInfo,
    current_session: Option<ActiveSession>,
    save_dir: PathBuf,
    pending_transfer: Arc<RwLock<Option<PendingTransfer>>>,
    received_files: Arc<RwLock<Vec<ReceivedFile>>>,
}

pub struct LocalSendServer {
    device: DeviceInfo,
    save_dir: PathBuf,
    handle: Option<JoinHandle<()>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    pending_transfer: Arc<RwLock<Option<PendingTransfer>>>,
    received_files: Arc<RwLock<Vec<ReceivedFile>>>,
}

impl LocalSendServer {
    pub fn new(
        alias: String,
        port: u16,
        save_dir: PathBuf,
        pending_transfer: Arc<RwLock<Option<PendingTransfer>>>,
        received_files: Arc<RwLock<Vec<ReceivedFile>>>,
    ) -> Self {
        let device = DeviceInfo {
            alias,
            version: PROTOCOL_VERSION.to_string(),
            device_model: Some(get_device_model()),
            device_type: Some(get_device_type()),
            fingerprint: generate_fingerprint(),
            port,
            protocol: Protocol::Http,
            download: false,
            ip: None,
        };
        Self {
            device,
            save_dir,
            handle: None,
            shutdown_tx: None,
            pending_transfer,
            received_files,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        let state = Arc::new(RwLock::new(ServerState {
            device: self.device.clone(),
            current_session: None,
            save_dir: self.save_dir.clone(),
            pending_transfer: Arc::clone(&self.pending_transfer),
            received_files: Arc::clone(&self.received_files),
        }));

        let router = create_router(state);
        let addr = format!("0.0.0.0:{}", self.device.port);
        let listener = TcpListener::bind(&addr).await?;
        log::info!("LocalSend HTTP server on {addr}");

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(shutdown_tx);

        let handle = tokio::spawn(async move {
            let server = axum::serve(listener, router).with_graceful_shutdown(async move {
                shutdown_rx.await.ok();
            });
            if let Err(e) = server.await {
                log::error!("LocalSend HTTP server error: {e}");
            }
        });

        self.handle = Some(handle);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            tx.send(()).ok();
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

fn create_router(state: Arc<RwLock<ServerState>>) -> Router {
    Router::new()
        .route("/api/localsend/v2/info", get(handle_info))
        .route("/api/localsend/v2/register", post(handle_register))
        .route("/api/localsend/v2/prepare-upload", post(handle_prepare_upload))
        .route("/api/localsend/v2/upload", post(handle_upload))
        .route("/api/localsend/v2/cancel", post(handle_cancel))
        .with_state(state)
}

async fn handle_info(State(state): State<Arc<RwLock<ServerState>>>) -> Response {
    let state = state.read().await;
    Json(state.device.clone()).into_response()
}

async fn handle_register(
    State(state): State<Arc<RwLock<ServerState>>>,
    Json(remote_device): Json<DeviceInfo>,
) -> Response {
    log::debug!("Register request from {:?}", remote_device.alias);
    let state = state.read().await;
    Json(state.device.clone()).into_response()
}

#[derive(Deserialize)]
struct PrepareUploadParams {
    #[serde(rename = "pin")]
    _pin: Option<String>,
}

/// Evict a stale current_session if it has been idle too long. Returns true if
/// a new session can proceed (no active session, or the old one was evicted).
async fn evict_stale_session(state: &RwLock<ServerState>) -> bool {
    let mut state = state.write().await;
    match &state.current_session {
        Some(session) if session.last_activity.elapsed() <= SESSION_IDLE_LIMIT => false,
        Some(_) => {
            state.current_session = None;
            true
        }
        None => true,
    }
}

async fn install_pending_session(
    state_ref: &Arc<RwLock<ServerState>>,
    request: &PrepareUploadRequest,
    session_id: &SessionId,
) -> oneshot::Receiver<bool> {
    let (response_tx, response_rx) = oneshot::channel();
    let mut state = state_ref.write().await;

    state.current_session = Some(ActiveSession {
        session_id: session_id.clone(),
        files: request.files.clone(),
        sender_alias: request.info.alias.clone(),
        last_activity: Instant::now(),
    });

    let pending = PendingTransfer {
        sender: request.info.clone(),
        files: request.files.clone(),
        response_tx,
    };

    let mut pending_guard = state.pending_transfer.write().await;
    *pending_guard = Some(pending);

    response_rx
}

async fn reject_pending_session(state_ref: &Arc<RwLock<ServerState>>) {
    let mut state = state_ref.write().await;
    let mut pending_guard = state.pending_transfer.write().await;
    *pending_guard = None;
    drop(pending_guard);
    state.current_session = None;
}

async fn handle_prepare_upload(
    State(state_ref): State<Arc<RwLock<ServerState>>>,
    Query(_params): Query<PrepareUploadParams>,
    Json(request): Json<PrepareUploadRequest>,
) -> Response {
    let session_id = SessionId::new();
    let mut files_map = HashMap::new();
    for file_id in request.files.keys() {
        files_map.insert(file_id.clone(), Token::new(&session_id, file_id));
    }

    if !evict_stale_session(&state_ref).await {
        log::warn!("Session already exists, rejecting new session");
        return StatusCode::CONFLICT.into_response();
    }

    let response_rx = install_pending_session(&state_ref, &request, &session_id).await;

    let accepted = matches!(
        tokio::time::timeout(ACCEPT_TIMEOUT, response_rx).await,
        Ok(Ok(true))
    );

    if !accepted {
        reject_pending_session(&state_ref).await;
        log::info!("Transfer rejected by user or timeout");
        return StatusCode::FORBIDDEN.into_response();
    }

    // Refresh activity after acceptance.
    {
        let mut state = state_ref.write().await;
        if let Some(s) = state.current_session.as_mut() {
            s.last_activity = Instant::now();
        }
    }

    Json(PrepareUploadResponse { session_id, files: files_map }).into_response()
}

#[derive(Deserialize)]
struct UploadParams {
    #[serde(rename = "sessionId")]
    session_id: SessionId,
    #[serde(rename = "fileId")]
    file_id: FileId,
    #[serde(rename = "token")]
    token: Token,
}

fn verify_upload<'a>(
    session: &'a ActiveSession,
    params: &UploadParams,
) -> std::result::Result<&'a FileMetadata, StatusCode> {
    if session.session_id != params.session_id {
        log::warn!(
            "Upload rejected: session ID mismatch. Expected {}, got {}",
            session.session_id,
            params.session_id
        );
        return Err(StatusCode::FORBIDDEN);
    }

    let expected_token = Token::new(&session.session_id, &params.file_id);
    if params.token.as_str() != expected_token.as_str() {
        log::warn!("Upload rejected: token mismatch");
        return Err(StatusCode::FORBIDDEN);
    }

    session.files.get(&params.file_id).ok_or_else(|| {
        log::warn!("Upload rejected: file ID {} not found in session", params.file_id);
        StatusCode::NOT_FOUND
    })
}

async fn write_upload(
    save_path: &std::path::Path,
    body: Bytes,
) -> std::result::Result<(), StatusCode> {
    if let Some(parent) = save_path.parent()
        && let Err(e) = tokio::fs::create_dir_all(parent).await
    {
        log::error!("Failed to create directory {parent:?}: {e}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    if let Err(e) = tokio::fs::write(save_path, body).await {
        log::error!("Failed to save file to {save_path:?}: {e}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Ok(())
}

async fn handle_upload(
    State(state_ref): State<Arc<RwLock<ServerState>>>,
    Query(params): Query<UploadParams>,
    body: Bytes,
) -> Response {
    let (file_name, save_path) = {
        let state = state_ref.read().await;
        let Some(session) = state.current_session.as_ref() else {
            log::warn!("Upload rejected: no active session");
            return StatusCode::FORBIDDEN.into_response();
        };
        let meta = match verify_upload(session, &params) {
            Ok(m) => m,
            Err(code) => return code.into_response(),
        };
        let file_name = meta.file_name.clone();
        let save_path = state.save_dir.join(&file_name);
        (file_name, save_path)
    };

    let body_len = body.len() as u64;
    if let Err(code) = write_upload(&save_path, body).await {
        return code.into_response();
    }
    log::info!("Received file: {save_path:?}");

    let mut state = state_ref.write().await;
    let sender = state
        .current_session
        .as_ref()
        .map(|s| s.sender_alias.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    {
        let mut files_list = state.received_files.write().await;
        files_list.push(ReceivedFile { file_name, size: body_len, sender });
    }

    if let Some(s) = state.current_session.as_mut() {
        s.last_activity = Instant::now();
        // Simplification: clear session after the final file. kvakk uses the
        // received_files notification to drive the UI, so a new transfer will
        // re-enter prepare-upload anyway.
        if s.files.len() <= 1 {
            state.current_session = None;
        }
    }

    StatusCode::OK.into_response()
}

#[derive(Deserialize)]
struct CancelParams {
    #[serde(rename = "sessionId")]
    session_id: SessionId,
}

async fn handle_cancel(
    State(state_ref): State<Arc<RwLock<ServerState>>>,
    Query(params): Query<CancelParams>,
) -> Response {
    let mut state = state_ref.write().await;
    if let Some(session) = state.current_session.as_ref()
        && session.session_id.as_str() == params.session_id.as_str()
    {
        state.current_session = None;
        log::info!("Session {} cancelled", params.session_id);
    }
    StatusCode::OK.into_response()
}
