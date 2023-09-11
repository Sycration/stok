use std::thread::{self, JoinHandle};

use tokio::runtime;

pub mod stok {
    tonic::include_proto!("stok");
}

enum ClientMessage {
    ConnectToServer(String),
}

enum ServerMessage {
    Error(String),
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct TemplateApp {
    #[serde(skip)]
    tx: std::sync::mpsc::Sender<ClientMessage>,
    #[serde(skip)]
    rx: std::sync::mpsc::Receiver<ServerMessage>,
    #[serde(skip)]
    thread: JoinHandle<()>,
    #[serde(skip)]
    error: Option<String>,

    address_string: String,
}

impl Default for TemplateApp {
    fn default() -> Self {
        let (client_msg_sender, client_msg_reciever) = std::sync::mpsc::channel();
        let (server_msg_sender, server_msg_reciever) = std::sync::mpsc::channel();
        let thread = thread::spawn(move || {
            let rt = runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let mut client = loop {
                    let addr: String = loop {
                        if let Ok(client_msg) = client_msg_reciever.recv() {
                            if let ClientMessage::ConnectToServer(addr) = client_msg {
                                break addr;
                            } else {
                                continue;
                            }
                        };
                    };

                    let client = stok::market_client::MarketClient::connect(addr).await;
                    match client {
                        Ok(c) => break c,
                        Err(e) => {
                            server_msg_sender.send(ServerMessage::Error(e.to_string()));
                            continue;
                        }
                    }
                };

                while let Ok(msg) = client_msg_reciever.recv() {
                    match msg {
                        ClientMessage::ConnectToServer(addr) => {
                            let new_client = stok::market_client::MarketClient::connect(addr).await;
                            match new_client {
                                Ok(c) => client = c,
                                Err(e) => {
                                    server_msg_sender.send(ServerMessage::Error(e.to_string()));
                                }
                            }
                        }
                    }
                }
            });
        });
        Self {
            address_string: "localhost:50051".to_string(),
            tx: client_msg_sender,
            rx: server_msg_reciever,
            thread,
            error: None,
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }
}

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let Self {
            address_string,
            tx,
            rx,
            thread,
            error,
        } = self;

        while let Ok(msg) = rx.try_recv() {
            match msg {
                ServerMessage::Error(e) => *error = Some(e),
            }
        }

        if let Some(e) = error {
            egui::Window::new("Error").show(ctx, |ui| {
                ui.monospace(e);
            });
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        _frame.close();
                    }
                });
            });
        });

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                if ui.button("connect").clicked() {
                    tx.send(ClientMessage::ConnectToServer(address_string.to_string()));
                }
                ui.text_edit_singleline(address_string);
            })
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's

            ui.heading("eframe template");
            egui::warn_if_debug_build(ui);
        });
    }
}

fn main() -> eframe::Result<()> {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "eframe template",
        native_options,
        Box::new(|cc| Box::new(TemplateApp::new(cc))),
    )
}
