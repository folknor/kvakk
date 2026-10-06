//! Wi-Fi LAN bandwidth upgrade for BLE-initiated inbound transfers.
//!
//! BLE moves tens of KB/s, so once the encrypted connection is up over the
//! weave socket, we (the receiver) offer a Wi-Fi LAN upgrade path:
//!
//! 1. Send UPGRADE_PATH_AVAILABLE (encrypted, over BLE) with our IPv4 and a
//!    freshly bound TCP port.
//! 2. The phone connects and sends a plaintext CLIENT_INTRODUCTION; we answer
//!    with a plaintext CLIENT_INTRODUCTION_ACK.
//! 3. Drain the BLE channel: send LAST_WRITE_TO_PRIOR_CHANNEL, answer the
//!    phone's LAST_WRITE with SAFE_TO_CLOSE_PRIOR_CHANNEL, and keep processing
//!    any normal frames it still sends over BLE meanwhile.
//! 4. Send a plaintext DISCONNECTION over BLE (without it the phone pauses the
//!    new channel for a ~10 s timeout), then swap the socket to TCP.
//!
//! Sequence numbers continue across the swap and encryption stays on, so the
//! rest of the transfer runs through the same state machine over TCP.
//! Behaviour verified against google/nearby by the upstream port.

use std::net::Ipv4Addr;
use std::time::Duration;

use anyhow::anyhow;
use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::{InboundRequest, SANE_FRAME_LENGTH};
use crate::hdl::MigratableStream;
use crate::location_nearby_connections::bandwidth_upgrade_negotiation_frame::upgrade_path_info::{
    Medium, WifiLanSocket,
};
use crate::location_nearby_connections::bandwidth_upgrade_negotiation_frame::{
    ClientIntroductionAck, EventType, UpgradePathInfo,
};
use crate::location_nearby_connections::{
    BandwidthUpgradeNegotiationFrame, DisconnectionFrame, OfflineFrame, V1Frame, offline_frame,
    v1_frame,
};
use crate::securemessage::SecureMessage;
use crate::utils::stream_read_exact;

/// How long the phone gets to connect to the offered TCP port.
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(15);
/// Per-frame wait while draining the BLE channel.
const DRAIN_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound on frames read while draining.
const MAX_DRAIN_FRAMES: usize = 16;
/// Time for the plaintext DISCONNECTION to flush through the weave bridge.
const DISCONNECT_FLUSH: Duration = Duration::from_millis(200);

fn v1_offline_frame(v1: V1Frame) -> OfflineFrame {
    OfflineFrame {
        version: Some(offline_frame::Version::V1.into()),
        v1: Some(v1),
    }
}

fn bwu_frame(negotiation: BandwidthUpgradeNegotiationFrame) -> OfflineFrame {
    v1_offline_frame(V1Frame {
        r#type: Some(v1_frame::FrameType::BandwidthUpgradeNegotiation.into()),
        bandwidth_upgrade_negotiation: Some(negotiation),
        ..Default::default()
    })
}

fn bwu_event(event: EventType) -> OfflineFrame {
    bwu_frame(BandwidthUpgradeNegotiationFrame {
        event_type: Some(event.into()),
        ..Default::default()
    })
}

fn upgrade_path_available(ip: Ipv4Addr, port: u16) -> OfflineFrame {
    bwu_frame(BandwidthUpgradeNegotiationFrame {
        event_type: Some(EventType::UpgradePathAvailable.into()),
        upgrade_path_info: Some(UpgradePathInfo {
            medium: Some(Medium::WifiLan.into()),
            wifi_lan_socket: Some(WifiLanSocket {
                ip_address: Some(ip.octets().to_vec()),
                wifi_port: Some(i32::from(port)),
            }),
            supports_client_introduction_ack: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    })
}

fn client_introduction_ack() -> OfflineFrame {
    bwu_frame(BandwidthUpgradeNegotiationFrame {
        event_type: Some(EventType::ClientIntroductionAck.into()),
        client_introduction_ack: Some(ClientIntroductionAck {}),
        ..Default::default()
    })
}

fn disconnection() -> OfflineFrame {
    v1_offline_frame(V1Frame {
        r#type: Some(v1_frame::FrameType::Disconnection.into()),
        disconnection: Some(DisconnectionFrame {
            request_safe_to_disconnect: Some(false),
            ack_safe_to_disconnect: Some(false),
        }),
        ..Default::default()
    })
}

/// The bandwidth-upgrade event carried by `frame`, if it is a BWU frame.
fn bwu_event_of(frame: &OfflineFrame) -> Option<EventType> {
    let v1 = frame.v1.as_ref()?;
    if v1.r#type() != v1_frame::FrameType::BandwidthUpgradeNegotiation {
        return None;
    }
    Some(v1.bandwidth_upgrade_negotiation.as_ref()?.event_type())
}

/// Read one plaintext `[4-byte BE length][frame]` message.
async fn read_plain_frame<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Vec<u8>, anyhow::Error> {
    let mut len_buf = [0u8; 4];
    stream_read_exact(stream, &mut len_buf).await?;
    let len = usize::try_from(u32::from_be_bytes(len_buf))?;
    if len == 0 || len > usize::try_from(SANE_FRAME_LENGTH)? {
        return Err(anyhow!("bad frame length {len}"));
    }
    let mut data = vec![0u8; len];
    stream_read_exact(stream, &mut data).await?;
    Ok(data)
}

/// Write one plaintext `[4-byte BE length][frame]` message.
async fn write_plain_frame<W: AsyncWrite + Unpin>(
    stream: &mut W,
    frame: &OfflineFrame,
) -> Result<(), anyhow::Error> {
    let data = frame.encode_to_vec();
    let len = u32::try_from(data.len())?;
    let mut buf = Vec::with_capacity(4 + data.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(&data);
    stream.write_all(&buf).await?;
    stream.flush().await?;
    Ok(())
}

/// The IPv4 address to offer: the first usable LAN address, the same set
/// `MDnsServer` advertises.
fn upgrade_ip() -> Result<Ipv4Addr, anyhow::Error> {
    crate::hdl::mdns::get_local_network_ips()
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no local IPv4 address for bandwidth upgrade"))
}

impl InboundRequest<MigratableStream> {
    /// Move the transfer from the BLE weave socket to Wi-Fi LAN. If the phone
    /// never connects, the session stays on BLE and this returns Ok.
    pub async fn do_bwu(&mut self) -> Result<(), anyhow::Error> {
        let ip = upgrade_ip()?;
        let listener = TcpListener::bind("0.0.0.0:0").await?;
        let port = listener.local_addr()?.port();
        info!("BWU: offering WIFI_LAN upgrade at {ip}:{port}");
        self.encrypt_and_send(&upgrade_path_available(ip, port)).await?;

        let Some(tcp) = Self::accept_upgrade(&listener).await? else {
            warn!("BWU: no TCP upgrade within {}s; staying on BLE", ACCEPT_TIMEOUT.as_secs());
            return Ok(());
        };

        self.encrypt_and_send(&bwu_event(EventType::LastWriteToPriorChannel)).await?;
        self.drain_prior_channel().await;

        // Plaintext, outside the encrypted sequence.
        if let Err(e) = self.send_frame(disconnection().encode_to_vec()).await {
            debug!("BWU: plaintext DISCONNECTION over BLE failed: {e}");
        }
        tokio::time::sleep(DISCONNECT_FLUSH).await;

        tcp.set_nodelay(true)?;
        self.socket = MigratableStream::Tcp(tcp);
        info!("BWU: upgraded to Wi-Fi LAN; payload continues over TCP");
        Ok(())
    }

    /// Accept the phone's TCP connection and complete the plaintext
    /// CLIENT_INTRODUCTION / ACK exchange. None on timeout.
    async fn accept_upgrade(listener: &TcpListener) -> Result<Option<TcpStream>, anyhow::Error> {
        let Ok(accepted) = tokio::time::timeout(ACCEPT_TIMEOUT, listener.accept()).await else {
            return Ok(None);
        };
        let (mut tcp, peer) = accepted?;
        info!("BWU: phone connected over TCP from {peer}");

        let intro = OfflineFrame::decode(&*read_plain_frame(&mut tcp).await?)?;
        if bwu_event_of(&intro) != Some(EventType::ClientIntroduction) {
            return Err(anyhow!("BWU: expected CLIENT_INTRODUCTION from {peer}"));
        }
        write_plain_frame(&mut tcp, &client_introduction_ack()).await?;
        Ok(Some(tcp))
    }

    /// Read the phone's remaining BLE frames until it is safe to close the
    /// prior channel. Non-BWU frames (the sharing handshake keeps going) are
    /// processed normally.
    async fn drain_prior_channel(&mut self) {
        for _ in 0..MAX_DRAIN_FRAMES {
            let frame = match tokio::time::timeout(DRAIN_FRAME_TIMEOUT, self.read_encrypted_frame()).await {
                Ok(Ok(frame)) => frame,
                Ok(Err(e)) => {
                    debug!("BWU drain: read failed: {e}");
                    return;
                }
                Err(_) => {
                    debug!("BWU drain: timed out waiting for the phone");
                    return;
                }
            };
            match bwu_event_of(&frame) {
                None => {
                    if let Err(e) = self.process_offline_frame(frame).await {
                        debug!("BWU drain: error processing frame: {e}");
                    }
                }
                Some(EventType::LastWriteToPriorChannel) => {
                    debug!("BWU drain: peer LAST_WRITE, sending SAFE_TO_CLOSE");
                    if let Err(e) = self.encrypt_and_send(&bwu_event(EventType::SafeToClosePriorChannel)).await {
                        debug!("BWU drain: SAFE_TO_CLOSE failed: {e}");
                    }
                }
                Some(EventType::SafeToClosePriorChannel) => {
                    debug!("BWU drain: peer SAFE_TO_CLOSE, done");
                    return;
                }
                Some(other) => debug!("BWU drain: event {other:?}"),
            }
        }
    }

    /// Read and decrypt one frame from the current channel.
    async fn read_encrypted_frame(&mut self) -> Result<OfflineFrame, anyhow::Error> {
        let data = read_plain_frame(&mut self.socket).await?;
        let smsg = SecureMessage::decode(&*data)?;
        self.decrypt_secure_message(&smsg).await
    }
}
