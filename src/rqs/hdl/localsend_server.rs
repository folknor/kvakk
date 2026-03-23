use std::path::PathBuf;
use std::sync::Arc;

use localsend_rs::protocol::ReceivedFile as LsReceivedFile;
use localsend_rs::server::PendingTransfer;
use localsend_rs::LocalSendServer;
use tokio::sync::{RwLock, broadcast};
use tokio_util::sync::CancellationToken;

use crate::DeviceType;
use crate::channel::{ChannelMessage, Message, MessageClient, TransferKind};
use crate::hdl::TransferState;
use crate::hdl::info::{TransferMetadata, TransferPayload, TransferPayloadKind};
use crate::utils::RemoteDeviceInfo;

const INNER_NAME: &str = "LocalSendServer";

pub struct LocalSendServerBridge {
    pending: Arc<RwLock<Option<PendingTransfer>>>,
    received: Arc<RwLock<Vec<LsReceivedFile>>>,
    server: LocalSendServer,
}

impl LocalSendServerBridge {
    pub fn new(
        alias: String,
        port: u16,
        save_dir: PathBuf,
    ) -> Result<Self, anyhow::Error> {
        let pending: Arc<RwLock<Option<PendingTransfer>>> = Arc::new(RwLock::new(None));
        let received: Arc<RwLock<Vec<LsReceivedFile>>> = Arc::new(RwLock::new(Vec::new()));

        let device = localsend_rs::protocol::DeviceInfo {
            alias,
            version: localsend_rs::protocol::PROTOCOL_VERSION.to_string(),
            device_model: Some(localsend_rs::get_device_model()),
            device_type: Some(localsend_rs::get_device_type()),
            fingerprint: localsend_rs::generate_fingerprint(),
            port,
            protocol: localsend_rs::protocol::Protocol::Http,
            download: false,
            ip: None,
        };

        let server = LocalSendServer::new_with_device(
            device,
            save_dir,
            false,
            Arc::clone(&pending),
            Arc::clone(&received),
        )?;

        Ok(Self {
            pending,
            received,
            server,
        })
    }

    pub async fn start(
        &mut self,
        message_sender: broadcast::Sender<ChannelMessage>,
        ctk: CancellationToken,
    ) -> Result<(), anyhow::Error> {
        self.server.start(None).await?;
        info!("{INNER_NAME}: HTTP server started");

        let pending = Arc::clone(&self.pending);
        let received = Arc::clone(&self.received);

        tokio::spawn(async move {
            poll_transfers(pending, received, message_sender, ctk).await;
        });

        Ok(())
    }
}

async fn poll_transfers(
    pending: Arc<RwLock<Option<PendingTransfer>>>,
    received: Arc<RwLock<Vec<LsReceivedFile>>>,
    message_sender: broadcast::Sender<ChannelMessage>,
    ctk: CancellationToken,
) {
    let mut poll_interval = tokio::time::interval(std::time::Duration::from_millis(200));
    let mut last_received_count: usize = 0;
    let mut active_transfer_id: Option<String> = None;

    loop {
        tokio::select! {
            _ = ctk.cancelled() => {
                info!("{INNER_NAME}: cancelled");
                break;
            }
            _ = poll_interval.tick() => {
                check_pending(&pending, &message_sender, &mut active_transfer_id).await;
                check_received(&received, &message_sender, &mut active_transfer_id, &mut last_received_count).await;
            }
        }
    }
}

async fn check_pending(
    pending: &Arc<RwLock<Option<PendingTransfer>>>,
    message_sender: &broadcast::Sender<ChannelMessage>,
    active_transfer_id: &mut Option<String>,
) {
    let mut guard = pending.write().await;
    let Some(transfer) = guard.take() else { return };

    let transfer_id = transfer.sender.fingerprint.clone();
    let file_names: Vec<String> = transfer
        .files
        .values()
        .map(|f| f.file_name.clone())
        .collect();
    let total_bytes: u64 = transfer.files.values().map(|f| f.size).sum();

    let metadata = TransferMetadata {
        id: transfer_id.clone(),
        source: Some(RemoteDeviceInfo {
            name: transfer.sender.alias.clone(),
            device_type: DeviceType::Unknown,
        }),
        pin_code: None,
        payload_kind: TransferPayloadKind::Files,
        payload_preview: None,
        payload: Some(TransferPayload::Files(file_names)),
        total_bytes,
        ack_bytes: 0,
    };

    // Auto-accept
    if transfer.response_tx.send(true).is_err() {
        warn!("{INNER_NAME}: failed to send accept response");
    }

    let msg = ChannelMessage {
        id: transfer_id.clone(),
        msg: Message::Client(MessageClient {
            kind: TransferKind::Inbound,
            state: Some(TransferState::ReceivingFiles),
            metadata: Some(metadata),
        }),
    };
    drop(message_sender.send(msg));
    *active_transfer_id = Some(transfer_id);
}

async fn check_received(
    received: &Arc<RwLock<Vec<LsReceivedFile>>>,
    message_sender: &broadcast::Sender<ChannelMessage>,
    active_transfer_id: &mut Option<String>,
    last_received_count: &mut usize,
) {
    let guard = received.read().await;
    let current_count = guard.len();
    if current_count <= *last_received_count {
        return;
    }
    *last_received_count = current_count;

    if let Some(id) = active_transfer_id.take() {
        let msg = ChannelMessage {
            id,
            msg: Message::Client(MessageClient {
                kind: TransferKind::Inbound,
                state: Some(TransferState::Finished),
                metadata: None,
            }),
        };
        drop(message_sender.send(msg));
    }
}
