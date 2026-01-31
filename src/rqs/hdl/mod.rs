use std::collections::HashMap;

use info::{InternalFileInfo, TransferMetadata};
use p256::{PublicKey, SecretKey};
use zeroize::Zeroize;

use crate::securegcm::ukey2_client_init::CipherCommitment;
use crate::sharing_nearby::wifi_credentials_metadata::SecurityType;
use crate::utils::RemoteDeviceInfo;

mod ble;
pub use ble::*;
#[cfg(target_os = "linux")]
mod blea;
#[cfg(target_os = "linux")]
pub use blea::*;
mod inbound;
pub use inbound::*;
pub mod info;
mod mdns_discovery;
pub use mdns_discovery::*;
mod mdns;
pub use mdns::*;
mod outbound;
pub use outbound::*;

use serde::{Deserialize, Serialize};


#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum TransferState {
    #[default]
    Initial,
    ReceivedConnectionRequest,
    SentUkeyServerInit,
    SentUkeyClientInit,
    SentUkeyClientFinish,
    SentPairedKeyEncryption,
    ReceivedUkeyClientFinish,
    SentConnectionResponse,
    SentPairedKeyResult,
    SentIntroduction,
    ReceivedPairedKeyResult,
    WaitingForUserConsent,
    ReceivingFiles,
    SendingFiles,
    Disconnected,
    Rejected,
    Cancelled,
    Finished,
}

#[derive(Debug)]
pub struct InnerState {
    pub id: String,
    pub server_seq: i32,
    pub client_seq: i32,
    pub encryption_done: bool,

    // Subject to be used-facing for progress, ...
    pub state: TransferState,
    pub remote_device_info: Option<RemoteDeviceInfo>,
    pub pin_code: Option<String>,
    pub transfer_metadata: Option<TransferMetadata>,
    pub transferred_files: HashMap<i64, InternalFileInfo>,

    // Everything needed for encryption/decryption/verif
    pub cipher_commitment: Option<CipherCommitment>,
    pub private_key: Option<SecretKey>,
    pub public_key: Option<PublicKey>,
    pub server_init_data: Option<Vec<u8>>,
    pub client_init_msg_data: Option<Vec<u8>>,
    pub ukey_client_finish_msg_data: Option<Vec<u8>>,
    pub decrypt_key: Option<Vec<u8>>,
    pub recv_hmac_key: Option<Vec<u8>>,
    pub encrypt_key: Option<Vec<u8>>,
    pub send_hmac_key: Option<Vec<u8>>,

    // Used to handle/track ingress transfer
    pub text_payload: Option<TextPayloadInfo>,
    // pub text_payload_id: i64,
    // pub text_is_url: bool,
    // pub wifi_ssid: Option<String>,
    pub payload_buffers: HashMap<i64, Vec<u8>>,
}

impl InnerState {
    /// Create a new InnerState with the given id and optional transfer metadata.
    pub fn new(id: String, transfer_metadata: Option<TransferMetadata>) -> Self {
        Self {
            id,
            server_seq: 0,
            client_seq: 0,
            encryption_done: true,
            state: TransferState::Initial,
            remote_device_info: None,
            pin_code: None,
            transfer_metadata,
            transferred_files: HashMap::new(),
            cipher_commitment: None,
            private_key: None,
            public_key: None,
            server_init_data: None,
            client_init_msg_data: None,
            ukey_client_finish_msg_data: None,
            decrypt_key: None,
            recv_hmac_key: None,
            encrypt_key: None,
            send_hmac_key: None,
            text_payload: None,
            payload_buffers: HashMap::new(),
        }
    }
}

impl Drop for InnerState {
    fn drop(&mut self) {
        // Zeroize all cryptographic keys to prevent memory leaks
        if let Some(ref mut key) = self.decrypt_key {
            key.zeroize();
        }
        if let Some(ref mut key) = self.recv_hmac_key {
            key.zeroize();
        }
        if let Some(ref mut key) = self.encrypt_key {
            key.zeroize();
        }
        if let Some(ref mut key) = self.send_hmac_key {
            key.zeroize();
        }
        // Zeroize intermediate key derivation data
        if let Some(ref mut data) = self.server_init_data {
            data.zeroize();
        }
        if let Some(ref mut data) = self.client_init_msg_data {
            data.zeroize();
        }
        if let Some(ref mut data) = self.ukey_client_finish_msg_data {
            data.zeroize();
        }

        // Clean up partial files from failed transfers
        if self.state != TransferState::Finished {
            for file_info in self.transferred_files.values() {
                if file_info.bytes_transferred > 0 && file_info.bytes_transferred < file_info.total_size {
                    // This is a partial file - attempt cleanup
                    if let Err(e) = std::fs::remove_file(&file_info.file_url) {
                        log::warn!("Failed to cleanup partial file {:?}: {e}", file_info.file_url);
                    } else {
                        log::info!("Cleaned up partial file: {:?}", file_info.file_url);
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum TextPayloadInfo {
    Url(i64),
    Text(i64),
    Wifi((i64, String, SecurityType)), // id, ssid, security type
}

#[derive(Debug, Clone)]
pub enum TextPayloadType {
    Url,
    Text,
    Wifi,
}

impl TextPayloadInfo {
    fn get_i64_value(&self) -> i64 {
        match self {
            TextPayloadInfo::Url(value)
            | TextPayloadInfo::Text(value)
            | TextPayloadInfo::Wifi((value, _, _)) => value.to_owned(),
        }
    }
}
