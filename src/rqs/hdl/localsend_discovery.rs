use std::collections::HashMap;
use std::time::Instant;

use localsend_rs::protocol::{DeviceInfo, Protocol};
use localsend_rs::{Discovery, MulticastDiscovery};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::DeviceType;

use super::{EndpointInfo, TransferProtocol};

const INNER_NAME: &str = "LocalSendDiscovery";

/// How often to re-announce presence (seconds)
const ANNOUNCE_INTERVAL_SECS: u64 = 5;

/// Devices not seen for this long are considered gone (seconds)
const DEVICE_TTL_SECS: u64 = 60;

pub struct LocalSendDiscoveryBridge {
    alias: String,
    port: u16,
    sender: broadcast::Sender<EndpointInfo>,
}

impl LocalSendDiscoveryBridge {
    pub fn new(alias: String, port: u16, sender: broadcast::Sender<EndpointInfo>) -> Self {
        Self {
            alias,
            port,
            sender,
        }
    }

    pub async fn run(self, ctk: CancellationToken) {
        info!("{INNER_NAME}: starting");

        let mut discovery = match MulticastDiscovery::new(
            self.alias,
            self.port,
            Protocol::Http,
        ) {
            Ok(d) => d,
            Err(e) => {
                error!("{INNER_NAME}: failed to create discovery: {e}");
                return;
            }
        };

        if let Err(e) = discovery.start().await {
            error!("{INNER_NAME}: failed to start: {e}");
            return;
        }

        // Subscribe to discovered devices
        let tx = self.sender.clone();
        let mut seen: HashMap<String, Instant> = HashMap::new();

        discovery.on_discovered(move |device: DeviceInfo| {
            let ei = device_to_endpoint(&device);
            drop(tx.send(ei));
        });

        // Periodic announce + TTL cleanup
        let mut announce_interval =
            tokio::time::interval(std::time::Duration::from_secs(ANNOUNCE_INTERVAL_SECS));

        loop {
            tokio::select! {
                _ = ctk.cancelled() => {
                    info!("{INNER_NAME}: cancelled, stopping");
                    discovery.stop();
                    break;
                }
                _ = announce_interval.tick() => {
                    if let Err(e) = discovery.announce_presence().await {
                        debug!("{INNER_NAME}: announce failed: {e}");
                    }

                    // TTL cleanup - remove stale devices
                    let now = Instant::now();
                    let expired: Vec<String> = seen
                        .iter()
                        .filter(|(_, last_seen)| now.duration_since(**last_seen).as_secs() > DEVICE_TTL_SECS)
                        .map(|(id, _)| id.clone())
                        .collect();

                    for id in expired {
                        seen.remove(&id);
                        drop(self.sender.send(EndpointInfo {
                            id,
                            present: Some(false),
                            protocol: TransferProtocol::LocalSend,
                            ..Default::default()
                        }));
                    }
                }
            }
        }
    }
}

fn device_to_endpoint(device: &DeviceInfo) -> EndpointInfo {
    let rtype = device.device_type.map(|dt| match dt {
        localsend_rs::protocol::DeviceType::Mobile => DeviceType::Phone,
        localsend_rs::protocol::DeviceType::Desktop => DeviceType::Laptop,
        _ => DeviceType::Unknown,
    });

    EndpointInfo {
        fullname: String::new(),
        id: device.fingerprint.clone(),
        name: Some(device.alias.clone()),
        ip: device.ip.clone(),
        port: Some(device.port.to_string()),
        rtype,
        present: Some(true),
        protocol: TransferProtocol::LocalSend,
        fingerprint: Some(device.fingerprint.clone()),
        ls_protocol: Some(device.protocol.to_string()),
    }
}
