use std::collections::HashMap;
use std::path::Path;

use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

use crate::DeviceType;
use crate::channel::{ChannelMessage, Message, MessageClient, TransferKind};
use crate::hdl::TransferState;
use crate::hdl::info::{TransferMetadata, TransferPayload, TransferPayloadKind};
use crate::localsend::{
    self, DeviceInfo, FileId, FileMetadata, LocalSendClient, PROTOCOL_VERSION, Protocol,
};
use crate::utils::RemoteDeviceInfo;

const INNER_NAME: &str = "LocalSendSender";

#[derive(Debug, Clone)]
pub struct LocalSendSendInfo {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub ls_protocol: String,
    pub fingerprint: String,
    pub files: Vec<String>,
}

pub async fn run_localsend_sender(
    mut rx: mpsc::Receiver<LocalSendSendInfo>,
    device_alias: String,
    message_sender: broadcast::Sender<ChannelMessage>,
    ctk: CancellationToken,
) {
    info!("{INNER_NAME}: ready");

    loop {
        tokio::select! {
            _ = ctk.cancelled() => {
                info!("{INNER_NAME}: cancelled");
                break;
            }
            Some(info) = rx.recv() => {
                let sender = message_sender.clone();
                let alias = device_alias.clone();
                tokio::spawn(async move {
                    handle_send(info, &alias, &sender).await;
                });
            }
        }
    }
}

async fn handle_send(
    info: LocalSendSendInfo,
    device_alias: &str,
    message_sender: &broadcast::Sender<ChannelMessage>,
) {
    let transfer_id = info.id.clone();

    let file_names: Vec<String> = info
        .files
        .iter()
        .filter_map(|f| Path::new(f).file_name().map(|n| n.to_string_lossy().to_string()))
        .collect();

    let protocol: Protocol = info.ls_protocol.as_str().into();
    let target = build_target_device(&info, protocol);
    let client = LocalSendClient::new(build_local_device(device_alias));

    let Some((files_meta, id_to_path)) = build_all_metadata(&info.files).await else {
        emit_state(message_sender, &transfer_id, TransferState::Cancelled);
        return;
    };

    let total_bytes: u64 = files_meta.values().map(|m| m.size).sum();
    let file_sizes: HashMap<FileId, u64> =
        files_meta.iter().map(|(id, m)| (id.clone(), m.size)).collect();
    emit_initial(message_sender, &transfer_id, &info.name, file_names, total_bytes);

    let upload_response = match client.prepare_upload(&target, files_meta).await {
        Ok(resp) => resp,
        Err(e) => {
            error!("{INNER_NAME}: prepare_upload failed: {e}");
            emit_state(message_sender, &transfer_id, TransferState::Rejected);
            return;
        }
    };

    if upload_response.session_id.is_empty() {
        emit_state(message_sender, &transfer_id, TransferState::Finished);
        return;
    }

    emit_state(message_sender, &transfer_id, TransferState::SendingFiles);

    let mut ack_bytes: u64 = 0;
    for (file_id, token) in &upload_response.files {
        let Some(path_str) = id_to_path.get(file_id) else {
            error!("{INNER_NAME}: no path for file_id {file_id}");
            continue;
        };

        let file_size = file_sizes.get(file_id).copied().unwrap_or(0);

        if let Err(e) = client
            .upload_file(
                &target,
                &upload_response.session_id,
                file_id,
                token,
                Path::new(path_str),
            )
            .await
        {
            error!("{INNER_NAME}: upload failed for {path_str}: {e}");
            emit_state(message_sender, &transfer_id, TransferState::Cancelled);
            return;
        }

        ack_bytes += file_size;
        emit_progress(message_sender, &transfer_id, total_bytes, ack_bytes);
    }

    emit_finished(message_sender, &transfer_id, total_bytes);
}

fn build_target_device(info: &LocalSendSendInfo, protocol: Protocol) -> DeviceInfo {
    DeviceInfo {
        alias: info.name.clone(),
        version: PROTOCOL_VERSION.to_string(),
        device_model: None,
        device_type: None,
        fingerprint: info.fingerprint.clone(),
        port: info.port,
        protocol,
        download: false,
        ip: Some(info.ip.clone()),
    }
}

fn build_local_device(alias: &str) -> DeviceInfo {
    DeviceInfo {
        alias: alias.to_string(),
        version: PROTOCOL_VERSION.to_string(),
        device_model: Some(localsend::get_device_model()),
        device_type: Some(localsend::get_device_type()),
        fingerprint: localsend::generate_fingerprint(),
        port: 0,
        protocol: Protocol::Http,
        download: false,
        ip: None,
    }
}

async fn build_all_metadata(
    files: &[String],
) -> Option<(HashMap<FileId, FileMetadata>, HashMap<FileId, String>)> {
    let mut files_meta = HashMap::new();
    let mut id_to_path = HashMap::new();

    for path_str in files {
        let path = Path::new(path_str);
        match localsend::build_file_metadata(path).await {
            Ok(meta) => {
                let file_id = meta.id.clone();
                id_to_path.insert(file_id.clone(), path_str.clone());
                files_meta.insert(file_id, meta);
            }
            Err(e) => error!("{INNER_NAME}: failed to build metadata for {path_str}: {e}"),
        }
    }

    if files_meta.is_empty() {
        error!("{INNER_NAME}: no valid files to send");
        return None;
    }

    Some((files_meta, id_to_path))
}

fn emit_initial(
    sender: &broadcast::Sender<ChannelMessage>,
    id: &str,
    device_name: &str,
    file_names: Vec<String>,
    total_bytes: u64,
) {
    let metadata = TransferMetadata {
        id: id.to_string(),
        source: Some(RemoteDeviceInfo {
            name: device_name.to_string(),
            device_type: DeviceType::Unknown,
        }),
        pin_code: None,
        payload_kind: TransferPayloadKind::Files,
        payload_preview: None,
        payload: Some(TransferPayload::Files(file_names)),
        total_bytes,
        ack_bytes: 0,
    };

    drop(sender.send(ChannelMessage {
        id: id.to_string(),
        msg: Message::Client(MessageClient {
            kind: TransferKind::Outbound,
            state: Some(TransferState::Initial),
            metadata: Some(metadata),
        }),
    }));
}

fn emit_state(sender: &broadcast::Sender<ChannelMessage>, id: &str, state: TransferState) {
    drop(sender.send(ChannelMessage {
        id: id.to_string(),
        msg: Message::Client(MessageClient {
            kind: TransferKind::Outbound,
            state: Some(state),
            metadata: None,
        }),
    }));
}

fn emit_progress(
    sender: &broadcast::Sender<ChannelMessage>,
    id: &str,
    total_bytes: u64,
    ack_bytes: u64,
) {
    drop(sender.send(ChannelMessage {
        id: id.to_string(),
        msg: Message::Client(MessageClient {
            kind: TransferKind::Outbound,
            state: Some(TransferState::SendingFiles),
            metadata: Some(TransferMetadata {
                id: id.to_string(),
                source: None,
                pin_code: None,
                payload_kind: TransferPayloadKind::Files,
                payload_preview: None,
                payload: None,
                total_bytes,
                ack_bytes,
            }),
        }),
    }));
}

fn emit_finished(sender: &broadcast::Sender<ChannelMessage>, id: &str, total_bytes: u64) {
    drop(sender.send(ChannelMessage {
        id: id.to_string(),
        msg: Message::Client(MessageClient {
            kind: TransferKind::Outbound,
            state: Some(TransferState::Finished),
            metadata: Some(TransferMetadata {
                id: id.to_string(),
                source: None,
                pin_code: None,
                payload_kind: TransferPayloadKind::Files,
                payload_preview: None,
                payload: None,
                total_bytes,
                ack_bytes: total_bytes,
            }),
        }),
    }));
}
