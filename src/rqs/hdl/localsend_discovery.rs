use std::collections::HashMap;
use std::time::Instant;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::DeviceType;
use crate::localsend::{DeviceInfo, DeviceType as LsDeviceType, MulticastDiscovery, Protocol};

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
        Self { alias, port, sender }
    }

    pub async fn run(self, ctk: CancellationToken) {
        info!("{INNER_NAME}: starting");

        let mut discovery = MulticastDiscovery::new(self.alias, self.port, Protocol::Http);
        let mut rx = discovery.subscribe();

        if let Err(e) = discovery.start().await {
            error!("{INNER_NAME}: failed to start: {e}");
            return;
        }

        let mut seen: HashMap<String, Instant> = HashMap::new();
        let mut announce_interval =
            tokio::time::interval(std::time::Duration::from_secs(ANNOUNCE_INTERVAL_SECS));

        loop {
            tokio::select! {
                _ = ctk.cancelled() => {
                    info!("{INNER_NAME}: cancelled, stopping");
                    discovery.stop();
                    break;
                }
                Ok(device) = rx.recv() => {
                    seen.insert(device.fingerprint.clone(), Instant::now());
                    drop(self.sender.send(device_to_endpoint(&device)));
                }
                _ = announce_interval.tick() => {
                    if let Err(e) = discovery.announce_presence().await {
                        debug!("{INNER_NAME}: announce failed: {e}");
                    }
                    expire_stale(&mut seen, &self.sender);
                }
            }
        }
    }
}

fn expire_stale(seen: &mut HashMap<String, Instant>, sender: &broadcast::Sender<EndpointInfo>) {
    let now = Instant::now();
    let expired: Vec<String> = seen
        .iter()
        .filter(|(_, last)| now.duration_since(**last).as_secs() > DEVICE_TTL_SECS)
        .map(|(id, _)| id.clone())
        .collect();

    for id in expired {
        seen.remove(&id);
        drop(sender.send(EndpointInfo {
            id,
            present: Some(false),
            protocol: TransferProtocol::LocalSend,
            ..Default::default()
        }));
    }
}

fn device_to_endpoint(device: &DeviceInfo) -> EndpointInfo {
    let rtype = device.device_type.map(|dt| match dt {
        LsDeviceType::Mobile => DeviceType::Phone,
        LsDeviceType::Desktop => DeviceType::Laptop,
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
