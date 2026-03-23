#[macro_use]
extern crate log;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, RwLock};

use anyhow::anyhow;
use channel::ChannelMessage;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use hdl::BleAdvertiser;
use hdl::MDnsDiscovery;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::hdl::BleListener;
use crate::hdl::MDnsServer;
use crate::hdl::{LocalSendDiscoveryBridge, LocalSendServerBridge};
use crate::manager::TcpServer;

pub mod channel;
pub mod errors;
pub mod hdl;
pub mod manager;
pub mod utils;

pub use hdl::{EndpointInfo, OutboundPayload, TransferProtocol, TransferState};
pub use hdl::localsend_send::LocalSendSendInfo;
pub use manager::SendInfo;
pub use utils::DeviceType;

/// Default LocalSend port
const LOCALSEND_PORT: u16 = 53317;

pub mod sharing_nearby {
    include!(concat!(env!("OUT_DIR"), "/sharing.nearby.rs"));
}

pub mod securemessage {
    include!(concat!(env!("OUT_DIR"), "/securemessage.rs"));
}

pub mod securegcm {
    include!(concat!(env!("OUT_DIR"), "/securegcm.rs"));
}

pub mod location_nearby_connections {
    include!(concat!(env!("OUT_DIR"), "/location.nearby.connections.rs"));
}

static CUSTOM_DOWNLOAD: LazyLock<RwLock<Option<PathBuf>>> = LazyLock::new(|| RwLock::new(None));
static DEVICE_NAME: LazyLock<RwLock<String>> = LazyLock::new(|| {
    RwLock::new(
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "Unknown device".into()),
    )
});

/// BLE status: 0 = initializing, 1 = active, 2 = unavailable
pub const BLE_INITIALIZING: u8 = 0;
pub const BLE_ACTIVE: u8 = 1;
pub const BLE_UNAVAILABLE: u8 = 2;

#[derive(Debug)]
pub struct RQS {
    tracker: Option<TaskTracker>,
    ctoken: Option<CancellationToken>,
    // Discovery token is different than ctoken because he is on his own
    // - can be cancelled while the ctoken is still active
    discovery_ctk: Option<CancellationToken>,

    // Only used to send the info "a nearby device is sharing"
    ble_sender: broadcast::Sender<()>,

    pub port_number: Option<u32>,

    pub message_sender: broadcast::Sender<ChannelMessage>,

    pub ble_status: Arc<AtomicU8>,
}

impl Default for RQS {
    fn default() -> Self {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "Unknown device".into());
        Self::new(None, None, Some(hostname))
    }
}

impl RQS {
    pub fn new(
        port_number: Option<u32>,
        download_path: Option<PathBuf>,
        device_name: Option<String>,
    ) -> Self {
        if let Ok(mut guard) = CUSTOM_DOWNLOAD.write() {
            *guard = download_path;
        }
        if let Ok(mut guard) = DEVICE_NAME.write()
            && let Some(device_name) = device_name
        {
            *guard = device_name.clone();
        }

        let (message_sender, _) = broadcast::channel(50);
        let (ble_sender, _) = broadcast::channel(5);

        Self {
            tracker: None,
            ctoken: None,
            discovery_ctk: None,
            ble_sender,
            port_number,
            message_sender,
            ble_status: Arc::new(AtomicU8::new(BLE_INITIALIZING)),
        }
    }

    pub async fn run(
        &mut self,
    ) -> Result<(mpsc::Sender<SendInfo>, mpsc::Sender<LocalSendSendInfo>, broadcast::Receiver<()>, bool), anyhow::Error> {
        let tracker = TaskTracker::new();
        let ctoken = CancellationToken::new();
        self.tracker = Some(tracker.clone());
        self.ctoken = Some(ctoken.clone());

        let endpoint_id = utils::get_endpoint_id();
        let tcp_listener =
            TcpListener::bind(format!("0.0.0.0:{}", self.port_number.unwrap_or(0))).await?;
        let binded_addr = tcp_listener.local_addr()?;
        info!("TcpListener on: {binded_addr}");

        // So the random port can be accessed from the user if needed.
        // This does have a difference in behaviour however when port_number is Some.
        // .stop() and .run() will reuse the port number instead of generating a new one.
        self.port_number = Some(binded_addr.port() as u32);

        // MPSC for the TcpServer
        let send_channel = mpsc::channel(10);
        // Start TcpServer in own "task"
        let mut server = TcpServer::new(
            endpoint_id,
            tcp_listener,
            self.message_sender.clone(),
            send_channel.1,
        )?;
        let ctk = ctoken.clone();
        tracker.spawn(async move { server.run(ctk).await });

        {
            let ble_sender = self.ble_sender.clone();
            let ble_status = Arc::clone(&self.ble_status);
            let ctk = ctoken.clone();
            tracker.spawn(async move {
                match BleListener::new(ble_sender).await {
                    Ok(ble) => {
                        ble_status.store(BLE_ACTIVE, Ordering::Relaxed);
                        drop(ble.run(ctk).await);
                    }
                    Err(err) => {
                        ble_status.store(BLE_UNAVAILABLE, Ordering::Relaxed);
                        warn!("BleListener: {err}");
                    }
                }
            });
        }

        // Start MDnsServer in own "task"
        let mut mdns = MDnsServer::new(
            endpoint_id,
            binded_addr.port(),
            self.ble_sender.subscribe(),
        )?;
        let ctk = ctoken.clone();
        tracker.spawn(async move { mdns.run(ctk).await });

        // Start LocalSend HTTP server for receiving files
        let device_name = self.get_device_name();
        let save_dir = utils::get_download_dir();
        let mut localsend_ok = false;
        match LocalSendServerBridge::new(device_name.clone(), LOCALSEND_PORT, save_dir) {
            Ok(mut ls_server) => {
                let msg_sender = self.message_sender.clone();
                let ctk = ctoken.clone();
                if let Err(e) = ls_server.start(msg_sender, ctk).await {
                    warn!("LocalSendServer failed to start: {e}");
                } else {
                    info!("LocalSendServer started on port {LOCALSEND_PORT}");
                    localsend_ok = true;
                }
            }
            Err(e) => warn!("LocalSendServer init failed: {e}"),
        }

        // Start LocalSend sender task
        let ls_send_channel = mpsc::channel(10);
        {
            let msg_sender = self.message_sender.clone();
            let ctk = ctoken.clone();
            let alias = device_name;
            let rx = ls_send_channel.1;
            tracker.spawn(async move {
                hdl::localsend_send::run_localsend_sender(rx, alias, msg_sender, ctk).await;
            });
        }

        tracker.close();

        Ok((send_channel.0, ls_send_channel.0, self.ble_sender.subscribe(), localsend_ok))
    }

    pub fn discovery(
        &mut self,
        sender: broadcast::Sender<EndpointInfo>,
    ) -> Result<(), anyhow::Error> {
        let tracker = self
            .tracker
            .as_ref()
            .ok_or_else(|| anyhow!("The service wasn't first started"))?;

        let ctk = CancellationToken::new();
        self.discovery_ctk = Some(ctk.clone());

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            let ctk_blea = ctk.clone();
            tracker.spawn(async move {
                let blea = match BleAdvertiser::new().await {
                    Ok(b) => b,
                    Err(e) => {
                        error!("Couldn't init BleAdvertiser: {e}");
                        return;
                    }
                };

                if let Err(e) = blea.run(ctk_blea).await {
                    error!("Couldn't start BleAdvertiser: {e}");
                }
            });
        }

        let discovery = MDnsDiscovery::new(sender.clone())?;
        let ctk_mdns = ctk.clone();
        tracker.spawn(async move { discovery.run(ctk_mdns).await });

        // Start LocalSend multicast discovery
        {
            let device_name = self.get_device_name();
            let ls_discovery = LocalSendDiscoveryBridge::new(
                device_name,
                LOCALSEND_PORT,
                sender,
            );
            let ctk_ls = ctk.clone();
            tracker.spawn(async move { ls_discovery.run(ctk_ls).await });
        }

        Ok(())
    }

    pub fn stop_discovery(&mut self) {
        if let Some(discovert_ctk) = &self.discovery_ctk {
            discovert_ctk.cancel();
            self.discovery_ctk = None;
        }
    }

    pub async fn stop(&mut self) {
        self.stop_discovery();

        if let Some(ctoken) = &self.ctoken {
            ctoken.cancel();
        }

        if let Some(tracker) = &self.tracker {
            // Inorder for TaskTracker::wait to return, close() must be called
            // and the count of tasks being watched should be 0 (i.e. they've all closed).
            //
            // If not, the TaskTracker may forever wait if task count is 0 when wait() was called
            tracker.close();
            tracker.wait().await;
        }

        self.ctoken = None;
        self.tracker = None;
    }

    // Setting None here will resume the default settings
    pub fn set_download_path(&self, p: Option<PathBuf>) {
        debug!("Setting the download path to {p:?}");
        if let Ok(mut guard) = CUSTOM_DOWNLOAD.write() {
            *guard = p;
        }
    }

    /// For this to properly take effect,
    /// `MdnsServer` would need to be reset which is done by `RQS::stop` followed by `RQS::run`.
    ///
    /// So only do this when no data transfer is going on.
    pub fn set_device_name(&self, name: String) {
        debug!("Setting the device name {name:?}");
        if let Ok(mut guard) = DEVICE_NAME.write() {
            *guard = name;
        }
    }

    pub fn get_device_name(&self) -> String {
        DEVICE_NAME
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "Unknown".to_string())
    }
}
