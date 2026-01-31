use std::net::Ipv4Addr;

use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};
use tokio::sync::broadcast::Receiver;
use tokio_util::sync::CancellationToken;

use crate::utils::{gen_mdns_endpoint_info, gen_mdns_name, DeviceType};
use crate::DEVICE_NAME;

/// Check if interface name belongs to a virtual/tunnel network
fn is_virtual_interface(name: &str) -> bool {
    let name_lower = name.to_lowercase();
    name_lower.starts_with("docker")
        || name_lower.starts_with("br-")
        || name_lower.starts_with("veth")
        || name_lower.starts_with("virbr")
        || name_lower.starts_with("tailscale")
        || name_lower.starts_with("tun")
        || name_lower.starts_with("tap")
}

/// Find all usable IPv4 addresses (excluding loopback/link-local/virtual networks)
fn get_local_network_ips() -> Vec<Ipv4Addr> {
    let mut ips = Vec::new();

    if let Ok(interfaces) = get_if_addrs::get_if_addrs() {
        for iface in interfaces {
            // Skip loopback
            if iface.is_loopback() {
                continue;
            }
            // Skip virtual/tunnel interfaces by name
            if is_virtual_interface(&iface.name) {
                continue;
            }
            // Skip non-IPv4
            let ip = match iface.ip() {
                std::net::IpAddr::V4(ip) => ip,
                _ => continue,
            };
            // Skip link-local (169.254.x.x)
            if ip.octets()[0] == 169 && ip.octets()[1] == 254 {
                continue;
            }
            // Skip virtualization networks (172.16.x.x - 172.31.x.x)
            // Covers Docker, WSL2, Hyper-V, VMware, VirtualBox, etc.
            if ip.octets()[0] == 172 && (16..=31).contains(&ip.octets()[1]) {
                continue;
            }
            // Skip Tailscale CGNAT range (100.64.0.0/10 = 100.64.x.x - 100.127.x.x)
            if ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]) {
                continue;
            }
            ips.push(ip);
        }
    }

    ips
}

const INNER_NAME: &str = "MDnsServer";

pub struct MDnsServer {
    daemon: ServiceDaemon,
    service_info: ServiceInfo,
    ble_receiver: Receiver<()>,
}

impl MDnsServer {
    pub fn new(
        endpoint_id: [u8; 4],
        service_port: u16,
        ble_receiver: Receiver<()>,
    ) -> Result<Self, anyhow::Error> {
        let service_info = Self::build_service(endpoint_id, service_port, DeviceType::Laptop)?;

        Ok(Self {
            daemon: ServiceDaemon::new()?,
            service_info,
            ble_receiver,
        })
    }

    pub async fn run(&mut self, ctk: CancellationToken) -> Result<(), anyhow::Error> {
        info!("{INNER_NAME}: service starting");
        let monitor = self.daemon.monitor()?;
        let ble_receiver = &mut self.ble_receiver;

        // Always register - this fork is always visible
        self.daemon.register(self.service_info.clone())?;

        loop {
            tokio::select! {
                _ = ctk.cancelled() => {
                    info!("{INNER_NAME}: tracker cancelled, breaking");
                    break;
                }
                r = monitor.recv_async() => {
                    match r {
                        Ok(_) => continue,
                        Err(err) => return Err(err.into()),
                    }
                },
                _ = ble_receiver.recv() => {
                    debug!("{INNER_NAME}: ble_receiver: got event");
                    // Android can sometimes not see the mDNS service if the service
                    // was running BEFORE Android started the Discovery phase for QuickShare.
                    // So resend a broadcast if there's an Android device sending.
                    self.daemon.register(self.service_info.clone())?;
                },
            }
        }

        // Unregister the mDNS service - we're shutting down
        let receiver = self.daemon.unregister(self.service_info.get_fullname())?;
        if let Ok(event) = receiver.recv() {
            info!("MDnsServer: service unregistered: {:?}", &event);
        }

        Ok(())
    }

    fn build_service(
        endpoint_id: [u8; 4],
        service_port: u16,
        device_type: DeviceType,
    ) -> Result<ServiceInfo, anyhow::Error> {
        let name = gen_mdns_name(endpoint_id);
        let hostname = format!("{name}.local.");
        let device_name = DEVICE_NAME
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to read device name: {e}"))?
            .clone();

        // Find all usable IPv4 addresses (local network + Tailscale)
        let local_ips = get_local_network_ips();
        info!("Broadcasting with: device_name={device_name}, host_name={hostname}, ips={local_ips:?}");

        let endpoint_info = gen_mdns_endpoint_info(device_type as u8, &device_name);
        let properties = [("n", endpoint_info)];

        // Pass IPs as comma-separated string, or empty for auto-detection
        let ip_str = if local_ips.is_empty() {
            String::new()
        } else {
            local_ips
                .iter()
                .map(|ip| ip.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };

        let mut si = ServiceInfo::new(
            "_FC9F5ED42C8A._tcp.local.",
            &name,
            &hostname,
            &ip_str,
            service_port,
            &properties[..],
        )?;

        // If no specific IPs were set, enable auto-detection but limit to IPv4
        if local_ips.is_empty() {
            si = si.enable_addr_auto();
        }

        // Only broadcast on IPv4 interfaces
        si.set_interfaces(vec![IfKind::IPv4]);

        Ok(si)
    }
}
