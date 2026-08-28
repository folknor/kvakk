use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use socket2::{Domain, Protocol as SocketProtocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::broadcast;

use super::client::LocalSendClient;
use super::device::{generate_fingerprint, get_device_model, get_device_type};
use super::error::{LocalSendError, Result};
use super::protocol::{
    AnnouncementMessage, DEFAULT_MULTICAST_ADDRESS, DEFAULT_MULTICAST_PORT, DeviceInfo,
    PROTOCOL_VERSION, Protocol,
};

pub struct MulticastDiscovery {
    local_device: DeviceInfo,
    client: LocalSendClient,
    socket: Option<Arc<UdpSocket>>,
    running: Arc<AtomicBool>,
    tx: broadcast::Sender<DeviceInfo>,
}

impl MulticastDiscovery {
    pub fn new(alias: String, port: u16, protocol: Protocol) -> Self {
        let device = DeviceInfo {
            alias,
            version: PROTOCOL_VERSION.to_string(),
            device_model: Some(get_device_model()),
            device_type: Some(get_device_type()),
            fingerprint: generate_fingerprint(),
            port,
            protocol,
            download: false,
            ip: None,
        };

        let (tx, _rx) = broadcast::channel(100);
        Self {
            client: LocalSendClient::new(device.clone()),
            local_device: device,
            socket: None,
            running: Arc::new(AtomicBool::new(false)),
            tx,
        }
    }

    /// Subscribe to discovered devices.
    pub fn subscribe(&self) -> broadcast::Receiver<DeviceInfo> {
        self.tx.subscribe()
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.running.swap(true, Ordering::Relaxed) {
            return Err(LocalSendError::network("Discovery already running"));
        }

        let bind_addr: SocketAddr = format!("0.0.0.0:{DEFAULT_MULTICAST_PORT}").parse()?;
        let socket = create_reusable_udp_socket(&bind_addr)?;
        let multicast_ipv4: Ipv4Addr = DEFAULT_MULTICAST_ADDRESS.parse().map_err(|_| {
            LocalSendError::network("Failed to parse multicast address as IPv4")
        })?;
        socket.join_multicast_v4(multicast_ipv4, Ipv4Addr::UNSPECIFIED)?;

        let socket_arc = Arc::new(socket);
        self.socket = Some(Arc::clone(&socket_arc));

        let tx = self.tx.clone();
        let client = self.client.clone();
        let running = Arc::clone(&self.running);
        let local_device = self.local_device.clone();

        tokio::spawn(async move {
            recv_loop(socket_arc, running, tx, client, local_device).await;
        });

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.socket = None;
    }

    pub async fn announce_presence(&self) -> Result<()> {
        let socket = self
            .socket
            .as_ref()
            .ok_or_else(|| LocalSendError::network("Discovery not started"))?;

        let announcement = announcement_from(&self.local_device, true);
        let msg = serde_json::to_string(&announcement)?;
        let buf = msg.as_bytes();
        let multicast_addr: SocketAddr =
            format!("{DEFAULT_MULTICAST_ADDRESS}:{DEFAULT_MULTICAST_PORT}").parse()?;

        // Send a few times with small delays to improve reliability on flaky networks.
        for delay_ms in [100u64, 500, 2000] {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            socket.send_to(buf, &multicast_addr).await?;
        }

        Ok(())
    }
}

async fn recv_loop(
    socket: Arc<UdpSocket>,
    running: Arc<AtomicBool>,
    tx: broadcast::Sender<DeviceInfo>,
    client: LocalSendClient,
    local_device: DeviceInfo,
) {
    let mut buf = vec![0u8; 65536];

    while running.load(Ordering::Relaxed) {
        let recv =
            tokio::time::timeout(Duration::from_secs(1), socket.recv_from(&mut buf)).await;
        let Ok(Ok((len, src))) = recv else { continue };
        if len == 0 {
            continue;
        }

        let Ok(msg) = std::str::from_utf8(&buf[..len]) else { continue };
        let Ok(announcement) = serde_json::from_str::<AnnouncementMessage>(msg) else {
            continue;
        };

        if announcement.fingerprint == local_device.fingerprint {
            continue;
        }

        let device = device_from_announcement(&announcement, src);
        let is_announcement =
            announcement.announce || announcement.announcement.unwrap_or(false);

        drop(tx.send(device.clone()));

        if is_announcement {
            let client = client.clone();
            let local_device = local_device.clone();
            let socket = Arc::clone(&socket);
            tokio::spawn(async move {
                respond_to_announcement(&client, &device, &local_device, &socket).await;
            });
        }
    }
}

fn announcement_from(device: &DeviceInfo, announcing: bool) -> AnnouncementMessage {
    AnnouncementMessage {
        alias: device.alias.clone(),
        version: device.version.clone(),
        device_model: device.device_model.clone(),
        device_type: device.device_type,
        fingerprint: device.fingerprint.clone(),
        port: device.port,
        protocol: device.protocol,
        download: device.download,
        announce: announcing,
        announcement: Some(announcing),
    }
}

fn device_from_announcement(announcement: &AnnouncementMessage, src: SocketAddr) -> DeviceInfo {
    DeviceInfo {
        alias: announcement.alias.clone(),
        version: announcement.version.clone(),
        device_model: announcement.device_model.clone(),
        device_type: announcement.device_type,
        fingerprint: announcement.fingerprint.clone(),
        port: announcement.port,
        protocol: announcement.protocol,
        download: announcement.download,
        ip: Some(src.ip().to_string()),
    }
}

async fn respond_to_announcement(
    client: &LocalSendClient,
    target_device: &DeviceInfo,
    local_device: &DeviceInfo,
    socket: &UdpSocket,
) {
    log::debug!(
        "Responding to announcement from {} ({:?})",
        target_device.alias,
        target_device.ip
    );

    // Try HTTP registration first. Fall back to UDP if it fails - common when
    // peers have strict firewalls or an incompatible JSON response shape.
    match client.register(target_device).await {
        Ok(_) => {
            log::debug!("Registered with {} via HTTP", target_device.alias);
            return;
        }
        Err(e) => log::debug!("HTTP registration failed ({e}), falling back to UDP..."),
    }

    let announcement = announcement_from(local_device, false);
    let Ok(msg) = serde_json::to_string(&announcement) else { return };
    let Ok(multicast_addr) =
        format!("{DEFAULT_MULTICAST_ADDRESS}:{DEFAULT_MULTICAST_PORT}").parse::<SocketAddr>()
    else {
        return;
    };
    if let Err(e) = socket.send_to(msg.as_bytes(), &multicast_addr).await {
        log::debug!("Failed to send UDP fallback response: {e}");
    }
}

/// Creates a UDP socket with port reuse enabled.
///
/// Critical for LocalSend: multiple processes may want to join the fixed
/// multicast group (53317) on the same host. SO_REUSEADDR (and SO_REUSEPORT on
/// Unix) lets the OS fan incoming multicast packets out to every participating
/// socket.
fn create_reusable_udp_socket(bind_addr: &SocketAddr) -> Result<UdpSocket> {
    let domain = if bind_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
    let socket = Socket::new(domain, Type::DGRAM, Some(SocketProtocol::UDP))
        .map_err(|e| LocalSendError::network(format!("Failed to create socket: {e}")))?;

    socket
        .set_reuse_address(true)
        .map_err(|e| LocalSendError::network(format!("Failed to set reuse_address: {e}")))?;

    #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    socket
        .set_reuse_port(true)
        .map_err(|e| LocalSendError::network(format!("Failed to set reuse_port: {e}")))?;

    socket
        .bind(&(*bind_addr).into())
        .map_err(|e| LocalSendError::network(format!("Failed to bind to {bind_addr}: {e}")))?;

    let std_socket: std::net::UdpSocket = socket.into();
    std_socket
        .set_nonblocking(true)
        .map_err(|e| LocalSendError::network(format!("Failed to set non-blocking: {e}")))?;

    UdpSocket::from_std(std_socket)
        .map_err(|e| LocalSendError::network(format!("Failed to convert to tokio socket: {e}")))
}

