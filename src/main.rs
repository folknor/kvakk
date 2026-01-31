//! RQuickShare - egui frontend

use std::sync::mpsc;
use std::thread;

use eframe::egui;
use rqs::channel::{ChannelMessage, Message, TransferAction};
use rqs::hdl::{EndpointInfo, TransferState};
use rqs::{OutboundPayload, SendInfo, RQS};
use tokio::sync::broadcast;

// Catppuccin Mocha palette as egui colors
mod theme {
    use eframe::egui::Color32;

    // Base colors
    pub const BASE: Color32 = Color32::from_rgb(30, 30, 46);
    pub const CRUST: Color32 = Color32::from_rgb(17, 17, 27);

    // Surface colors
    pub const SURFACE0: Color32 = Color32::from_rgb(49, 50, 68);
    pub const SURFACE1: Color32 = Color32::from_rgb(69, 71, 90);

    // Text colors
    pub const TEXT: Color32 = Color32::from_rgb(205, 214, 244);
    pub const SUBTEXT1: Color32 = Color32::from_rgb(186, 194, 222);
    pub const SUBTEXT0: Color32 = Color32::from_rgb(166, 173, 200);
    pub const OVERLAY0: Color32 = Color32::from_rgb(108, 112, 134);

    // Accent colors
    pub const BLUE: Color32 = Color32::from_rgb(137, 180, 250);
    pub const GREEN: Color32 = Color32::from_rgb(166, 227, 161);
    pub const YELLOW: Color32 = Color32::from_rgb(249, 226, 175);
    pub const RED: Color32 = Color32::from_rgb(243, 139, 168);
    pub const MAUVE: Color32 = Color32::from_rgb(203, 166, 247);
}

fn configure_style(ctx: &egui::Context) {
    catppuccin_egui::set_theme(ctx, catppuccin_egui::MOCHA);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    ctx.set_style(style);
}

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 520.0])
            .with_min_inner_size([320.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "RQuickShare",
        options,
        Box::new(|cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(RQuickShareApp::new(cc)))
        }),
    )
}

struct Transfer {
    id: String,
    device_name: String,
    file_names: Vec<String>,
    pin_code: Option<String>,
    state: TransferState,
    total_bytes: u64,
    ack_bytes: u64,
}

struct RQuickShareApp {
    rx: mpsc::Receiver<ChannelMessage>,
    cmd_tx: Option<broadcast::Sender<ChannelMessage>>,
    send_tx: Option<tokio::sync::mpsc::Sender<SendInfo>>,
    transfers: Vec<Transfer>,
    endpoints: Vec<EndpointInfo>,
    outbound_files: Vec<String>,
    status_message: String,
}

impl RQuickShareApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = mpsc::channel();
        let (init_tx, init_rx) = std::sync::mpsc::channel::<(
            broadcast::Sender<ChannelMessage>,
            tokio::sync::mpsc::Sender<SendInfo>,
        )>();

        let ctx = cc.egui_ctx.clone();

        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(async move {
                let mut rqs = RQS::default();
                let message_sender = rqs.message_sender.clone();
                let mut receiver = rqs.message_sender.subscribe();

                match rqs.run().await {
                    Ok((sender_file, _ble_receiver)) => {
                        drop(init_tx.send((message_sender, sender_file)));

                        loop {
                            match receiver.recv().await {
                                Ok(msg) => {
                                    if let Message::Client(_) = &msg.msg {
                                        drop(tx.send(msg));
                                        ctx.request_repaint();
                                    }
                                }
                                Err(e) => {
                                    log::error!("Receiver error: {e}");
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to start RQS: {e}");
                    }
                }
            });
        });

        let (cmd_tx, send_tx) = init_rx
            .recv()
            .map(|(cmd, send)| (Some(cmd), Some(send)))
            .unwrap_or((None, None));

        Self {
            rx,
            cmd_tx,
            send_tx,
            transfers: Vec::new(),
            endpoints: Vec::new(),
            outbound_files: Vec::new(),
            status_message: String::from("Ready"),
        }
    }

    fn process_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            if let Message::Client(client) = &msg.msg {
                let state = client.state.clone().unwrap_or(TransferState::Initial);

                if let Some(transfer) = self.transfers.iter_mut().find(|t| t.id == msg.id) {
                    transfer.state = state.clone();
                    if let Some(meta) = &client.metadata {
                        transfer.total_bytes = meta.total_bytes;
                        transfer.ack_bytes = meta.ack_bytes;
                    }
                    if matches!(state, TransferState::Finished | TransferState::Cancelled | TransferState::Rejected) {
                        transfer.state = state;
                    }
                } else if let Some(meta) = &client.metadata {
                    let file_names = meta.payload.as_ref().map_or_else(Vec::new, |p| {
                        match p {
                            rqs::hdl::info::TransferPayload::Files(files) => files.clone(),
                            rqs::hdl::info::TransferPayload::Text(t) => vec![format!("Text: {}", t.chars().take(50).collect::<String>())],
                            rqs::hdl::info::TransferPayload::Url(u) => vec![format!("URL: {u}")],
                            rqs::hdl::info::TransferPayload::Wifi { ssid, .. } => vec![format!("WiFi: {ssid}")],
                        }
                    });

                    self.transfers.push(Transfer {
                        id: msg.id.clone(),
                        device_name: meta.source.as_ref().map_or_else(|| "Unknown".to_string(), |s| s.name.clone()),
                        file_names,
                        pin_code: meta.pin_code.clone(),
                        state,
                        total_bytes: meta.total_bytes,
                        ack_bytes: meta.ack_bytes,
                    });
                }
            }
        }

        self.transfers.retain(|t| !matches!(t.state, TransferState::Disconnected));
    }

    fn send_action(&self, id: &str, action: TransferAction) {
        if let Some(cmd_tx) = &self.cmd_tx {
            let msg = ChannelMessage {
                id: id.to_string(),
                msg: Message::Lib { action },
            };
            if let Err(e) = cmd_tx.send(msg) {
                log::error!("Failed to send action: {e}");
            }
        }
    }

    fn clear_transfer(&mut self, id: &str) {
        self.transfers.retain(|t| t.id != id);
    }

    fn draw_header(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::new()
                .fill(theme::CRUST)
                .inner_margin(egui::Margin::symmetric(16, 12)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("RQuickShare")
                        .size(20.0)
                        .strong()
                        .color(theme::TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(&self.status_message)
                            .size(13.0)
                            .color(theme::SUBTEXT0));
                    });
                });
            });
    }

    fn draw_transfers_section(&mut self, ui: &mut egui::Ui) {
        if self.transfers.is_empty() {
            return;
        }

        ui.add_space(4.0);
        ui.label(egui::RichText::new("Transfers")
            .size(16.0)
            .strong()
            .color(theme::TEXT));
        ui.add_space(8.0);

        let transfers_snapshot: Vec<_> = self.transfers.iter().map(|t| {
            (t.id.clone(), t.device_name.clone(), t.file_names.clone(),
             t.pin_code.clone(), t.state.clone(), t.total_bytes, t.ack_bytes)
        }).collect();

        let mut to_clear = Vec::new();

        for (id, device_name, file_names, pin_code, state, total_bytes, ack_bytes) in transfers_snapshot {
            self.draw_transfer_card(ui, &id, &device_name, &file_names, &pin_code, &state, total_bytes, ack_bytes, &mut to_clear);
            ui.add_space(8.0);
        }

        for id in to_clear {
            self.clear_transfer(&id);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_transfer_card(&self, ui: &mut egui::Ui, id: &str, device_name: &str,
                          file_names: &[String], pin_code: &Option<String>,
                          state: &TransferState, total_bytes: u64, ack_bytes: u64,
                          to_clear: &mut Vec<String>) {
        egui::Frame::new()
            .fill(theme::SURFACE0)
            .stroke(egui::Stroke::new(1.0, theme::SURFACE1))
            .corner_radius(10)
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(device_name)
                        .strong()
                        .color(theme::TEXT));
                    if let Some(pin) = pin_code {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(format!("PIN: {pin}"))
                                .size(12.0)
                                .color(theme::MAUVE));
                        });
                    }
                });

                ui.add_space(6.0);

                for file in file_names {
                    ui.label(egui::RichText::new(format!("  {file}"))
                        .size(13.0)
                        .color(theme::SUBTEXT1));
                }

                ui.add_space(8.0);
                self.draw_transfer_state(ui, id, state, total_bytes, ack_bytes, to_clear);
            });
    }

    #[allow(clippy::cast_precision_loss)]
    fn draw_transfer_state(&self, ui: &mut egui::Ui, id: &str, state: &TransferState,
                           total_bytes: u64, ack_bytes: u64, to_clear: &mut Vec<String>) {
        match state {
            TransferState::WaitingForUserConsent => {
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(
                        egui::RichText::new("Accept").color(theme::CRUST))
                        .fill(theme::GREEN)
                    ).clicked() {
                        self.send_action(id, TransferAction::ConsentAccept);
                    }
                    ui.add_space(8.0);
                    if ui.add(egui::Button::new(
                        egui::RichText::new("Decline").color(theme::TEXT))
                        .fill(theme::SURFACE1)
                    ).clicked() {
                        self.send_action(id, TransferAction::ConsentDecline);
                    }
                });
            }
            TransferState::ReceivingFiles | TransferState::SendingFiles => {
                let progress = if total_bytes > 0 {
                    ack_bytes as f32 / total_bytes as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(progress)
                    .show_percentage()
                    .fill(theme::BLUE));
                ui.add_space(6.0);
                if ui.add(egui::Button::new(
                    egui::RichText::new("Cancel").size(12.0).color(theme::SUBTEXT0))
                    .fill(theme::SURFACE1)
                ).clicked() {
                    self.send_action(id, TransferAction::TransferCancel);
                }
            }
            TransferState::Finished => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Transfer complete!")
                        .color(theme::GREEN));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Clear").clicked() {
                            to_clear.push(id.to_string());
                        }
                    });
                });
            }
            TransferState::Cancelled => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Cancelled").color(theme::YELLOW));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Clear").clicked() {
                            to_clear.push(id.to_string());
                        }
                    });
                });
            }
            TransferState::Rejected => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Rejected").color(theme::RED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Clear").clicked() {
                            to_clear.push(id.to_string());
                        }
                    });
                });
            }
            _ => {
                ui.label(egui::RichText::new(format!("State: {state:?}"))
                    .size(12.0)
                    .color(theme::OVERLAY0));
            }
        }
    }

    fn draw_send_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Send Files")
            .size(16.0)
            .strong()
            .color(theme::TEXT));
        ui.add_space(8.0);

        if ui.add(egui::Button::new(
            egui::RichText::new("Select Files...").color(theme::CRUST))
            .fill(theme::BLUE)
            .min_size(egui::vec2(140.0, 36.0))
        ).clicked()
            && let Some(paths) = rfd::FileDialog::new().pick_files()
        {
            self.outbound_files = paths.iter().map(|p| p.display().to_string()).collect();
            self.status_message = format!("Selected {} file(s)", self.outbound_files.len());
        }

        if self.outbound_files.is_empty() {
            ui.add_space(12.0);
            ui.label(egui::RichText::new("No files selected")
                .size(13.0)
                .color(theme::OVERLAY0));
            return;
        }

        ui.add_space(12.0);

        egui::Frame::new()
            .fill(theme::SURFACE0)
            .stroke(egui::Stroke::new(1.0, theme::SURFACE1))
            .corner_radius(10)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Selected files:")
                    .size(13.0)
                    .color(theme::SUBTEXT0));
                ui.add_space(4.0);
                for file in &self.outbound_files {
                    let filename = std::path::Path::new(file)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(file);
                    ui.label(egui::RichText::new(format!("  {filename}"))
                        .size(13.0)
                        .color(theme::TEXT));
                }
            });

        ui.add_space(12.0);

        if !self.endpoints.is_empty() {
            self.draw_endpoint_buttons(ui);
        } else {
            ui.label(egui::RichText::new("Searching for nearby devices...")
                .size(13.0)
                .italics()
                .color(theme::OVERLAY0));
        }

        ui.add_space(8.0);
        if ui.add(egui::Button::new(
            egui::RichText::new("Clear Selection").size(13.0).color(theme::SUBTEXT0))
            .fill(theme::SURFACE1)
        ).clicked() {
            self.outbound_files.clear();
            self.status_message = String::from("Ready");
        }
    }

    fn draw_endpoint_buttons(&self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Send to:")
            .size(13.0)
            .color(theme::SUBTEXT0));
        ui.add_space(4.0);

        for endpoint in &self.endpoints {
            if ui.add(egui::Button::new(
                egui::RichText::new(endpoint.name.as_deref().unwrap_or("Unknown"))
                    .color(theme::TEXT))
                .fill(theme::SURFACE1)
            ).clicked()
                && let (Some(send_tx), Some(ip), Some(port)) = (&self.send_tx, &endpoint.ip, &endpoint.port)
            {
                let info = SendInfo {
                    id: endpoint.id.clone(),
                    name: endpoint.name.clone().unwrap_or_else(|| "Unknown".to_string()),
                    addr: format!("{ip}:{port}"),
                    ob: OutboundPayload::Files(self.outbound_files.clone()),
                };
                let tx = send_tx.clone();
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Runtime::new() {
                        rt.block_on(async {
                            drop(tx.send(info).await);
                        });
                    }
                });
            }
        }
    }
}

impl eframe::App for RQuickShareApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_messages();

        self.draw_header(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::new()
                .fill(theme::BASE)
                .inner_margin(egui::Margin::same(16)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_transfers_section(ui);
                    ui.add_space(20.0);
                    self.draw_send_section(ui);
                });
            });

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
