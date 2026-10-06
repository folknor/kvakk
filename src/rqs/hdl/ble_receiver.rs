//! Quick Share receiver discovery over BLE (service UUID 0xFEF3), Linux/BlueZ.
//!
//! Since Quick Share's AirDrop-compatibility update, phones (Pixel, Galaxy
//! S26 with "Share with Apple devices") drop off Wi-Fi when the share sheet
//! opens and only discover receivers over Bluetooth. A receiver that is only
//! reachable over mDNS + TCP is invisible to them. This module makes kvakk
//! discoverable and connectable over BLE:
//!
//! 1. `ReceiverAdvertiser` advertises a Nearby Connections endpoint under
//!    0xFEF3, carrying the same endpoint id as the mDNS service name.
//! 2. `ReceiverGattServer` serves the 0xFEF3 GATT service the phone connects
//!    to after the user picks us: an advertisement read characteristic plus
//!    the two weave socket characteristics (see `weave`).
//! 3. Each weave session runs an `InboundRequest` over an in-memory duplex,
//!    offering a Wi-Fi LAN bandwidth upgrade once the connection is encrypted,
//!    so file bytes go over TCP rather than BLE.
//!
//! Ported from martinalderson/rquickshare `feat/ble-receiver-connect-back`.

use std::sync::Arc;
use std::time::Duration;

use bluer::adv::Advertisement;
use bluer::gatt::local::{
    Application, Characteristic, CharacteristicNotifier, CharacteristicNotify,
    CharacteristicNotifyMethod, CharacteristicRead, CharacteristicWrite,
    CharacteristicWriteMethod, Service,
};
use bluer::{Adapter, Uuid, UuidExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf};
use tokio::sync::Mutex;
use tokio::sync::broadcast::Sender;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;

use super::weave::{self, EntrySplitter, Fragmenter, Incoming, NEARBY_SHARING_HASH, Reassembler};
use super::{InboundRequest, MigratableStream};
use crate::channel::ChannelMessage;
use crate::errors::AppError;

const ADV_INNER_NAME: &str = "ReceiverAdvertiser";
const GATT_INNER_NAME: &str = "ReceiverGattServer";

/// Nearby Connections copresence service, used for both advert and GATT.
const QS_SERVICE_UUID: u16 = 0xFEF3;
/// Advertisement slot 0: the phone reads our full advertisement here.
const ADV_SLOT0_UUID: &str = "00000000-0000-3000-8000-000000000000";
/// Weave "ToPeripheral": the phone writes packets to us.
const WEAVE_TO_PERIPHERAL_UUID: &str = "00000100-0004-1000-8000-001a11000101";
/// Weave "FromPeripheral": we notify packets to the phone.
const WEAVE_FROM_PERIPHERAL_UUID: &str = "00000100-0004-1000-8000-001a11000102";

/// A connectable advertising set is consumed when a phone connects, so it is
/// re-registered on this interval to stay discoverable.
const READVERTISE_INTERVAL: Duration = Duration::from_secs(30);
const ADVERTISE_RETRY: Duration = Duration::from_secs(3);
/// Fast advertising interval; BlueZ defaults to roughly 1-2 s.
const ADV_MIN_INTERVAL: Duration = Duration::from_millis(100);
const ADV_MAX_INTERVAL: Duration = Duration::from_millis(150);

/// Capacity of the in-memory pipe between the weave bridge and `InboundRequest`.
const DUPLEX_CAPACITY: usize = 64 * 1024;

// The segments below were captured from a Pixel 9 Pro advertising in
// "Everyone" mode and are reused verbatim; the phone does not validate them
// for discovery. Only the endpoint id, device name and lengths vary.

/// endpoint_info identity: 2-byte salt + 14-byte metadata-key hash.
const EINFO_IDENTITY: [u8; 16] = [
    0x4a, 0x22, 0x71, 0x16, 0x9c, 0x15, 0x99, 0xa2, 0x44, 0xaf, 0x44, 0xb0, 0x17, 0x9c, 0x0f, 0x23,
];
/// Connections advertisement trailer: bluetooth MAC (6) + extra (2).
const CONN_MAC_EXTRA: [u8; 8] = [0xfc, 0x41, 0x16, 0xb6, 0x17, 0x20, 0x00, 0x00];
/// Mediums advertisement trailer: device token (2) + extra (1) + Nearby
/// Presence data elements.
const MEDIUMS_TRAILER: [u8; 69] = [
    0x62, 0xf1, 0x03, 0x00, 0x82, 0x3f, 0xa0, 0x17, 0xfd, 0xf1, 0x70, 0x59, 0x6e, 0x1e, 0xd3, 0x4d,
    0xe0, 0x92, 0x56, 0x4d, 0x66, 0xd4, 0x29, 0x0f, 0x0f, 0x8f, 0x15, 0x05, 0x34, 0x7b, 0x13, 0x23,
    0x01, 0xea, 0x7f, 0x92, 0xa8, 0xd8, 0xd4, 0x61, 0x84, 0x15, 0x05, 0x3f, 0x00, 0x00, 0x84, 0x15,
    0x06, 0x2d, 0x00, 0x00, 0x84, 0x15, 0x04, 0x7f, 0x1f, 0x00, 0x84, 0x15, 0x07, 0x2d, 0x1f, 0x00,
    0x83, 0x15, 0x01, 0x15, 0x7c,
];

/// Build the 0xFEF3 service data advertising us as a Quick Share receiver.
///
/// `endpoint_id` must be the same 4 bytes `MDnsServer` puts in the mDNS
/// instance name, so BLE and Wi-Fi LAN resolve to one endpoint.
pub fn receiver_service_data(endpoint_id: [u8; 4], device_type: u8, device_name: &str) -> Vec<u8> {
    // Nearby Share endpoint info (same content as the mDNS "n" TXT record):
    // header version(3b)=1 | visibility(1b)=0 visible | device_type(3b) | reserved,
    // 16-byte identity, then the length-prefixed plaintext name.
    let name = &device_name.as_bytes()[..device_name.len().min(255)];
    let mut einfo = vec![(1 << 5) | ((device_type & 0x7) << 1)];
    einfo.extend_from_slice(&EINFO_IDENTITY);
    einfo.push(u8::try_from(name.len()).unwrap_or(u8::MAX));
    einfo.extend_from_slice(name);

    // Connections advertisement: version(3b)=1 | PCP(5b)=3, service hash,
    // endpoint id, endpoint info.
    let mut data = vec![0x23];
    data.extend_from_slice(&NEARBY_SHARING_HASH);
    data.extend_from_slice(&endpoint_id);
    data.push(u8::try_from(einfo.len()).unwrap_or(u8::MAX));
    data.extend_from_slice(&einfo);
    data.extend_from_slice(&CONN_MAC_EXTRA);

    // Mediums BLE advertisement: version(3b)=2 | socket_version(3b)=2 | fast=0,
    // service hash, big-endian data length, data, trailer.
    let mut sd = vec![0x48];
    sd.extend_from_slice(&NEARBY_SHARING_HASH);
    sd.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_be_bytes());
    sd.extend_from_slice(&data);
    sd.extend_from_slice(&MEDIUMS_TRAILER);
    sd
}

async fn powered_adapter() -> Result<Adapter, anyhow::Error> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;
    Ok(adapter)
}

#[derive(Debug, Clone)]
pub struct ReceiverAdvertiser {
    adapter: Arc<Adapter>,
    service_data: Vec<u8>,
}

impl ReceiverAdvertiser {
    pub async fn new(service_data: Vec<u8>) -> Result<Self, anyhow::Error> {
        Ok(Self {
            adapter: Arc::new(powered_adapter().await?),
            service_data,
        })
    }

    pub async fn run(&self, ctk: CancellationToken) -> Result<(), anyhow::Error> {
        info!(
            "{ADV_INNER_NAME}: advertising Quick Share receiver (0x{QS_SERVICE_UUID:04X}, {} bytes) on {} ({})",
            self.service_data.len(),
            self.adapter.name(),
            self.adapter.address().await?
        );

        let uuid = Uuid::from_u16(QS_SERVICE_UUID);
        loop {
            let adv = Advertisement {
                // Connectable, matching how phones advertise as receivers.
                advertisement_type: bluer::adv::Type::Peripheral,
                service_data: [(uuid, self.service_data.clone())].into(),
                discoverable: Some(true),
                min_interval: Some(ADV_MIN_INTERVAL),
                max_interval: Some(ADV_MAX_INTERVAL),
                ..Default::default()
            };
            let wait = match self.adapter.advertise(adv).await {
                Ok(handle) => {
                    let cancelled = tokio::select! {
                        () = ctk.cancelled() => true,
                        () = tokio::time::sleep(READVERTISE_INTERVAL) => false,
                    };
                    drop(handle);
                    if cancelled {
                        info!("{ADV_INNER_NAME}: tracker cancelled, returning");
                        return Ok(());
                    }
                    continue;
                }
                Err(e) => {
                    warn!("{ADV_INNER_NAME}: advertise failed ({e}); retrying");
                    ADVERTISE_RETRY
                }
            };
            tokio::select! {
                () = ctk.cancelled() => return Ok(()),
                () = tokio::time::sleep(wait) => {}
            }
        }
    }
}

/// Weave packets written by the phone, shared with the notify-side session.
type PacketReceiver = Arc<Mutex<UnboundedReceiver<Vec<u8>>>>;

pub struct ReceiverGattServer {
    adapter: Arc<Adapter>,
    advertisement: Vec<u8>,
    sender: Sender<ChannelMessage>,
}

impl ReceiverGattServer {
    pub async fn new(
        advertisement: Vec<u8>,
        sender: Sender<ChannelMessage>,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            adapter: Arc::new(powered_adapter().await?),
            advertisement,
            sender,
        })
    }

    pub async fn run(&self, ctk: CancellationToken) -> Result<(), anyhow::Error> {
        let (pkt_tx, pkt_rx) = unbounded_channel::<Vec<u8>>();
        let app = self.application(pkt_tx, Arc::new(Mutex::new(pkt_rx)))?;

        info!(
            "{GATT_INNER_NAME}: registering GATT service 0x{QS_SERVICE_UUID:04X} (slot0 {} bytes + weave socket)",
            self.advertisement.len()
        );
        let handle = self.adapter.serve_gatt_application(app).await?;
        ctk.cancelled().await;
        info!("{GATT_INNER_NAME}: tracker cancelled, returning");
        drop(handle);

        Ok(())
    }

    fn application(
        &self,
        pkt_tx: UnboundedSender<Vec<u8>>,
        pkt_rx: PacketReceiver,
    ) -> Result<Application, anyhow::Error> {
        let advert = self.advertisement.clone();
        let sender = self.sender.clone();

        let slot0 = Characteristic {
            uuid: ADV_SLOT0_UUID.parse()?,
            read: Some(CharacteristicRead {
                read: true,
                fun: Box::new(move |_req| {
                    let advert = advert.clone();
                    Box::pin(async move {
                        debug!("{GATT_INNER_NAME}: slot0 advertisement read ({} bytes)", advert.len());
                        Ok(advert)
                    })
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let to_peripheral = Characteristic {
            uuid: WEAVE_TO_PERIPHERAL_UUID.parse()?,
            write: Some(CharacteristicWrite {
                write: true,
                // Write-with-response only: the phone then sends packets one at
                // a time, so BlueZ delivers them to us in order.
                write_without_response: false,
                method: CharacteristicWriteMethod::Fun(Box::new(move |value, _req| {
                    let pkt_tx = pkt_tx.clone();
                    Box::pin(async move {
                        drop(pkt_tx.send(value));
                        Ok(())
                    })
                })),
                ..Default::default()
            }),
            ..Default::default()
        };

        let from_peripheral = Characteristic {
            uuid: WEAVE_FROM_PERIPHERAL_UUID.parse()?,
            notify: Some(CharacteristicNotify {
                notify: true,
                indicate: true,
                method: CharacteristicNotifyMethod::Fun(Box::new(move |notifier| {
                    let pkt_rx = Arc::clone(&pkt_rx);
                    let sender = sender.clone();
                    Box::pin(async move {
                        weave_session(notifier, pkt_rx, sender).await;
                    })
                })),
                ..Default::default()
            }),
            ..Default::default()
        };

        Ok(Application {
            services: vec![Service {
                uuid: Uuid::from_u16(QS_SERVICE_UUID),
                primary: true,
                characteristics: vec![slot0, to_peripheral, from_peripheral],
                ..Default::default()
            }],
            ..Default::default()
        })
    }
}

/// Wait for the phone's weave CONNECTION_REQUEST and return the packet size.
/// Returns None if the phone ends the notify subscription first.
async fn await_conn_request(
    notifier: &CharacteristicNotifier,
    rx: &mut UnboundedReceiver<Vec<u8>>,
) -> Option<u16> {
    let stopped = notifier.stopped();
    tokio::pin!(stopped);
    loop {
        let pkt = tokio::select! {
            () = &mut stopped => return None,
            pkt = rx.recv() => pkt?,
        };
        if let Some(size) = weave::parse_conn_request(&pkt) {
            return Some(size);
        }
    }
}

/// Run the inbound handshake over one end of the duplex, upgrading the payload
/// to Wi-Fi LAN once the connection is encrypted.
fn spawn_inbound(stream: DuplexStream, sender: Sender<ChannelMessage>) {
    tokio::spawn(async move {
        let mut ir = InboundRequest::new(MigratableStream::Ble(stream), "ble-weave".to_string(), sender);
        ir.enable_bwu();
        loop {
            if let Err(e) = ir.handle().await {
                if !matches!(e.downcast_ref(), Some(AppError::NotAnError)) {
                    debug!("{GATT_INNER_NAME}: weave inbound ended: {e}");
                }
                break;
            }
            if ir.take_bwu_pending()
                && let Err(e) = ir.do_bwu().await
            {
                warn!("{GATT_INNER_NAME}: bandwidth upgrade failed, staying on BLE: {e}");
            }
        }
    });
}

/// Bridges one weave BLE socket to an `InboundRequest`: answers the
/// connection request, then shuttles `[len][OfflineFrame]` bytes both ways.
async fn weave_session(
    mut notifier: CharacteristicNotifier,
    pkt_rx: PacketReceiver,
    sender: Sender<ChannelMessage>,
) {
    // One weave session at a time. A previous session ends as soon as the
    // phone stops its notify subscription, releasing the packet receiver.
    let mut rx = pkt_rx.lock().await;
    info!("{GATT_INNER_NAME}: weave notify session open");

    let Some(packet_size) = await_conn_request(&notifier, &mut rx).await else {
        return;
    };
    if let Err(e) = notifier.notify(weave::conn_confirm(packet_size)).await {
        error!("{GATT_INNER_NAME}: weave conn-confirm failed: {e}");
        return;
    }
    info!("{GATT_INNER_NAME}: weave connected (packet size {packet_size})");

    let (inbound_side, weave_side) = tokio::io::duplex(DUPLEX_CAPACITY);
    spawn_inbound(inbound_side, sender);
    let (weave_rd, weave_wr) = tokio::io::split(weave_side);

    shuttle(&mut notifier, &mut rx, weave_rd, weave_wr, Fragmenter::new(packet_size)).await;
    info!("{GATT_INNER_NAME}: weave session ended");
}

async fn shuttle(
    notifier: &mut CharacteristicNotifier,
    rx: &mut UnboundedReceiver<Vec<u8>>,
    mut weave_rd: ReadHalf<DuplexStream>,
    mut weave_wr: WriteHalf<DuplexStream>,
    mut fragmenter: Fragmenter,
) {
    let mut reassembler = Reassembler::default();
    let mut splitter = EntrySplitter::default();
    let mut rbuf = [0u8; 2048];
    let stopped = notifier.stopped();
    tokio::pin!(stopped);

    loop {
        tokio::select! {
            () = &mut stopped => {
                debug!("{GATT_INNER_NAME}: phone ended the notify subscription");
                break;
            }
            maybe_pkt = rx.recv() => {
                let Some(pkt) = maybe_pkt else { break };
                match reassembler.push(&pkt) {
                    Incoming::Pending => {}
                    Incoming::Close => {
                        debug!("{GATT_INNER_NAME}: weave peer closed the socket");
                        break;
                    }
                    Incoming::Control(msg) => {
                        debug!("{GATT_INNER_NAME}: weave control {}", hex::encode(&msg));
                    }
                    Incoming::Data(data) => {
                        if weave_wr.write_all(&data).await.is_err() {
                            break;
                        }
                    }
                }
            }
            r = weave_rd.read(&mut rbuf) => {
                let n = match r {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                for entry in splitter.push(&rbuf[..n]) {
                    for pkt in fragmenter.fragment(&entry) {
                        if let Err(e) = notifier.notify(pkt).await {
                            error!("{GATT_INNER_NAME}: weave notify failed: {e}");
                            return;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_data_layout() {
        let sd = receiver_service_data(*b"ABCD", 3, "dm6");
        assert_eq!(sd[0], 0x48);
        assert_eq!(&sd[1..4], &NEARBY_SHARING_HASH);
        let data_len = usize::try_from(u32::from_be_bytes([sd[4], sd[5], sd[6], sd[7]])).unwrap_or(0);
        assert_eq!(sd.len(), 8 + data_len + MEDIUMS_TRAILER.len());

        let data = &sd[8..8 + data_len];
        assert_eq!(data[0], 0x23);
        assert_eq!(&data[4..8], b"ABCD");
        let einfo_len = usize::from(data[8]);
        let einfo = &data[9..9 + einfo_len];
        assert_eq!(einfo[0], 0x26); // version 1, visible, laptop
        assert_eq!(usize::from(einfo[17]), 3);
        assert_eq!(&einfo[18..], b"dm6");
        assert_eq!(&data[9 + einfo_len..], &CONN_MAC_EXTRA);
    }
}
