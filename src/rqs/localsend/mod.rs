//! Vendored and trimmed LocalSend protocol implementation.
//!
//! Originally based on <https://github.com/CrossCopy/localsend-rs> (MIT).
//! Reduced to only the surface kvakk uses: multicast discovery, an HTTP-only
//! server that auto-accepts transfers, and a client for sending files.

pub mod client;
pub mod device;
pub mod discovery;
pub mod error;
pub mod protocol;
pub mod server;

pub use client::LocalSendClient;
pub use device::{build_file_metadata, generate_fingerprint, get_device_model, get_device_type};
pub use discovery::MulticastDiscovery;
pub use error::{LocalSendError, Result};
pub use protocol::{
    DEFAULT_MULTICAST_ADDRESS, DEFAULT_MULTICAST_PORT, DeviceInfo, DeviceType, FileId,
    FileMetadata, PROTOCOL_VERSION, PrepareUploadResponse, Protocol, ReceivedFile, SessionId,
    Token,
};
pub use server::{LocalSendServer, PendingTransfer};
