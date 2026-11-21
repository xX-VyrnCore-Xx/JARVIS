use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::{json, Value};

use crate::client::ApiClient;
use jarvis_launcher::{launch, LaunchConfig, RunningCore};

const NAV: [(&str, &str, bool); 14] = [
    ("dashboard", "Plancia", true),
    ("chat", "Chat", false),
    ("tasks", "Attività", false),
    ("automations", "Automazioni", true),
    ("memory", "Memoria", true),
    ("skills", "Competenze", true),
    ("workspace", "Workspace", true),
    ("connectors", "Connettori", true),
    ("devices", "Dispositivi", true),
    ("security", "Sicurezza", true),
    ("audit", "Audit", true),
    ("system", "Sistema", true),
    ("models", "Modelli AI", true),
    ("updates", "Aggiornamenti", true),
];

#[derive(Clone, Debug)]
struct ChatMessage {
    role: &'static str,
    text: String,
}

enum Event {
    Connected(Box<Result<Connection, String>>),
    Loaded(String, Result<Value, String>),
    History(Result<Value, String>),
    ActionDone(Result<Value, String>),
    PairingCreated(Result<Value, String>),
    CloudConfigured(Result<Value, String>, bool),
    ChatDelta(String),
    ChatFinished(Result<Value, String>),
    Notifications(Result<Value, String>),
}

struct Connection {
    client: ApiClient,
    core: Option<RunningCore>,
    remote: bool,
}

#[derive(Clone, Copy)]
enum Mutation {
    Post,
    Patch,
    Delete,
}

pub struct JarvisApp {
    tx: Sender<Event>,
    rx: Receiver<Event>,
    client: Option<Arc<ApiClient>>,
    core: Option<RunningCore>,
    remote: bool,
    current: String,
    data: Option<Value>,
    error: Option<String>,
    loading: bool,
    last_load: Instant,
    chat_input: String,
    chat: Vec<ChatMessage>,
    history_loading: bool,
    streaming: String,
    chat_busy: bool,
    notifications: Vec<Value>,
    notifications_error: Option<String>,
    confirmations: Vec<Value>,
    memory_edits: HashMap<String, String>,
    known_notifications: HashSet<String>,
    notifications_primed: bool,
    focused: bool,
    remote_url: String,
    pair_code: String,
    pairing: bool,
    pair_error: Option<String>,
    pending_delete: Option<(String, String)>,
    device_invite: Option<Value>,
    invite_confirm: bool,
    invite_creating: bool,
    cloud_provider: String,
    cloud_api_key: String,
    cloud_action_busy: bool,
    cloud_action_error: Option<String>,
    cloud_action_notice: Option<String>,
    cloud_remove_confirm: bool,
    github_update_token: String,
    update_remove_confirm: bool,
}

impl JarvisApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.window_fill = egui::Color32::from_rgb(12, 21, 34);
        style.visuals.panel_fill = egui::Color32::from_rgb(13, 24, 39);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(16, 185, 190);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(28, 93, 112);
        style.spacing.item_spacing = egui::vec2(10.0, 9.0);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);

        let (tx, rx) = mpsc::channel();
        let app_tx = tx.clone();
        std::thread::spawn(move || {
            let result = connect_core();
            let _ = app_tx.send(Event::Connected(Box::new(result)));
        });

        Self {
            tx,
            rx,
            client: None,
            core: None,
            remote: false,
            current: "dashboard".into(),
            data: None,
            error: None,
            loading: false,
            last_load: Instant::now() - Duration::from_secs(60),
            chat_input: String::new(),
            chat: Vec::new(),
            history_loading: false,
            streaming: String::new(),
            chat_busy: false,
            notifications: Vec::new(),
            notifications_error: None,
            confirmations: Vec::new(),
            memory_edits: HashMap::new(),
            known_notifications: HashSet::new(),
            notifications_primed: false,
            focused: true,
            remote_url: configured_remote_url(),
            pair_code: String::new(),
            pairing: false,
            pair_error: None,
            pending_delete: None,
            device_invite: None,
            invite_confirm: true,
            invite_creating: false,
            cloud_provider: "ollama_cloud".into(),
            cloud_api_key: String::new(),
            cloud_action_busy: false,
            cloud_action_error: None,
            cloud_action_notice: None,
            cloud_remove_confirm: false,
            github_update_token: String::new(),
            update_remove_confirm: false,
        }
    }

    fn receive_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Connected(result) => match *result {
                    Ok(connection) => {
                        self.remote = connection.remote;
                        self.core = connection.core;
                        self.client = Some(Arc::new(connection.client));
                        self.pairing = false;
                        self.error = None;
                        if self.remote {
                            self.current = "chat".into();
                        }
                        self.start_notification_poll(ctx);
                        self.history_loading = true;
                        self.refresh(ctx);
                    }
                    Err(error) => {
                        self.pairing = false;
                        self.error = Some(error);
                    }
                },
                Event::Loaded(page, result) if page == self.current => {
                    self.loading = false;
                    match result {
                        Ok(data) => {
                            self.data = Some(data);
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Event::Loaded(_, _) => {
                    self.loading = false;
                    self.refresh(ctx);
                }
                Event::History(Ok(history)) => {
                    self.history_loading = false;
                    self.chat = parse_history(&history);
                    ctx.request_repaint();
                }
                Event::History(Err(error)) => {
                    self.history_loading = false;
                    self.error = Some(error);
                }
                Event::ActionDone(result) => {
                    self.loading = false;
                    match result {
                        Ok(_) => self.error = None,
                        Err(error) => self.error = Some(error),
                    }
                    self.data = None;
                    self.last_load = Instant::now() - Duration::from_secs(60);
                    self.refresh(ctx);
                }
                Event::PairingCreated(result) => {
                    self.invite_creating = false;
                    match result {
                        Ok(invite) => self.device_invite = Some(invite),
                        Err(error) => self.error = Some(error),
                    }
                    self.data = None;
                    self.last_load = Instant::now() - Duration::from_secs(60);
                    self.refresh(ctx);
                }
                Event::CloudConfigured(result, removed) => {
                    self.cloud_action_busy = false;
                    self.cloud_remove_confirm = false;
                    match result {
                        Ok(status) => {
                            if self.current == "models" {
                                self.data = Some(status);
                            } else {
                                self.data = None;
                                self.last_load = Instant::now() - Duration::from_secs(60);
                                self.refresh(ctx);
                            }
                            self.cloud_action_error = None;
                            self.cloud_action_notice = Some(if removed {
                                "La chiave del provider è stata rimossa.".into()
                            } else {
                                "Chiave salvata. Prova il provider dalla Chat; l'inferenza remota può comportare costi.".into()
                            });
                        }
                        Err(error) => {
                            self.cloud_action_notice = None;
                            self.cloud_action_error = Some(error);
                        }
                    }
                    ctx.request_repaint();
                }
                Event::ChatDelta(delta) => {
                    self.streaming.push_str(&delta);
                    ctx.request_repaint();
                }
                Event::ChatFinished(Ok(result)) => {
                    let reply = result["reply"].as_str().unwrap_or("").to_string();
                    if !reply.is_empty() {
                        if let Some(last) = self.chat.last_mut().filter(|m| m.role == "assistant") {
                            last.text = reply;
                        } else {
                            self.chat.push(ChatMessage {
                                role: "assistant",
                                text: reply,
                            });
                        }
                    }
                    if let Some(confirmation) = result.get("confirmation") {
                        self.confirmations.push(confirmation.clone());
                    }
                    self.streaming.clear();
                    self.chat_busy = false;
                    self.load_history();
                    self.refresh(ctx);
                }
                Event::ChatFinished(Err(error)) => {
                    self.streaming.clear();
                    self.chat_busy = false;
                    self.chat.push(ChatMessage {
                        role: "assistant",
                        text: format!("Errore: {error}"),
                    });
                }
                Event::Notifications(Ok(data)) => {
                    self.notifications_error = None;
                    let items = data["items"].as_array().cloned().unwrap_or_default();
                    if self.notifications_primed && !self.focused {
                        for item in &items {
                            let id = item["notification_id"].as_str().unwrap_or_default();
                            if self.known_notifications.insert(id.to_string()) {
                                show_notification(item);
                            }
                        }
                    } else {
                        self.known_notifications
                            .extend(items.iter().filter_map(|item| {
                                item["notification_id"].as_str().map(str::to_owned)
                            }));
                    }
                    self.notifications_primed = true;
                    self.notifications = items;
                    ctx.request_repaint();
                }
                Event::Notifications(Err(error)) => self.notifications_error = Some(error),
            }
        }
    }

    fn start_notification_poll(&self, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || loop {
            if tx
                .send(Event::Notifications(
                    client.get("/api/notifications?limit=20"),
                ))
                .is_err()
            {
                break;
            }
            ctx.request_repaint();
            std::thread::sleep(Duration::from_secs(5));
        });
    }

    fn load_history(&self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::History(client.get("/api/conversation?limit=100")));
        });
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if self.loading {
            return;
        }
        self.loading = true;
        self.last_load = Instant::now();
        let page = self.current.clone();
        let tx = self.tx.clone();
        let path = endpoint(&page);
        std::thread::spawn(move || {
            let result = client.get(path);
            let _ = tx.send(Event::Loaded(page, result));
        });
        ctx.request_repaint();
    }

    fn send_chat(&mut self) {
        let message = self.chat_input.trim().to_string();
        if message.is_empty() || self.chat_busy || self.history_loading {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.chat_input.clear();
        self.chat.push(ChatMessage {
            role: "user",
            text: message.clone(),
        });
        self.chat.push(ChatMessage {
            role: "assistant",
            text: String::new(),
        });
        self.chat_busy = true;
        self.streaming.clear();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = client.chat_stream(&message, |delta| {
                let _ = tx.send(Event::ChatDelta(delta.to_string()));
            });
            let _ = tx.send(Event::ChatFinished(result));
        });
    }

    fn act(&mut self, ctx: &egui::Context, path: String, body: Option<Value>) {
        self.mutate(ctx, Mutation::Post, path, body);
    }

    fn create_device_invite(&mut self, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let mut scopes = vec!["chat"];
        if self.invite_confirm {
            scopes.push("confirm");
        }
        let tx = self.tx.clone();
        self.invite_creating = true;
        std::thread::spawn(move || {
            let result = client.post("/api/devices/pairing", Some(&json!({"scopes": scopes})));
            let _ = tx.send(Event::PairingCreated(result));
        });
        ctx.request_repaint();
    }

    fn configure_cloud(&mut self, ctx: &egui::Context, remove_key: bool) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let provider = self.cloud_provider.clone();
        let body = if remove_key {
            json!({"provider": provider, "remove_key": true})
        } else {
            let key = std::mem::take(&mut self.cloud_api_key);
            if key.is_empty()
                || key.len() > 1024
                || key.bytes().any(|byte| !byte.is_ascii_graphic())
            {
                self.cloud_action_error = Some(
                    "Inserisci una chiave API valida, senza spazi o caratteri di controllo.".into(),
                );
                return;
            }
            json!({"provider": provider, "key": key})
        };
        let tx = self.tx.clone();
        self.cloud_action_busy = true;
        self.cloud_action_error = None;
        self.cloud_action_notice = None;
        self.cloud_remove_confirm = false;
        std::thread::spawn(move || {
            let result = client.post("/api/cloud", Some(&body));
            let _ = tx.send(Event::CloudConfigured(result, remove_key));
        });
        ctx.request_repaint();
    }

    fn mutate(
        &mut self,
        ctx: &egui::Context,
        mutation: Mutation,
        path: String,
        body: Option<Value>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = match mutation {
                Mutation::Post => client.post(&path, body.as_ref()),
                Mutation::Patch => body
                    .as_ref()
                    .ok_or_else(|| "Corpo della richiesta mancante".to_string())
                    .and_then(|body| client.patch(&path, body)),
                Mutation::Delete => client.delete(&path),
            };
            let _ = tx.send(Event::ActionDone(result));
        });
        self.loading = true;
        ctx.request_repaint();
    }

    fn navigation(&mut self, ui: &mut egui::Ui, compact: bool, ctx: &egui::Context) {
        for (id, label, owner_only) in NAV {
            if (self.remote && owner_only) || (compact && id != "chat") {
                continue;
            }
            let selected = self.current == id;
            let button = egui::Button::new(
                egui::RichText::new(if compact {
                    label.chars().next().unwrap_or('•').to_string()
                } else {
                    label.to_string()
                })
                .color(if selected {
                    egui::Color32::from_rgb(118, 236, 229)
                } else {
                    egui::Color32::from_rgb(204, 218, 231)
                }),
            )
            .fill(if selected {
                egui::Color32::from_rgb(18, 54, 70)
            } else {
                egui::Color32::TRANSPARENT
            })
            .corner_radius(egui::CornerRadius::same(8));
            if ui.add_sized([ui.available_width(), 38.0], button).clicked() {
                self.current = id.into();
                self.data = None;
                self.refresh(ctx);
            }
        }
    }

    fn show_chat(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        if self.history_loading {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Carico la cronologia…");
            });
        }
        egui::ScrollArea::vertical()
            .id_salt("chat-scroll")
            .stick_to_bottom(true)
            .max_height(ui.available_height() - 75.0)
            .show(ui, |ui| {
                if self.chat.is_empty() {
                    ui.add_space(40.0);
                    ui.heading("Come posso aiutarti?");
                    ui.label(
                        "Scrivi un comando o una domanda. Le azioni sensibili richiedono conferma.",
                    );
                    ui.add_space(20.0);
                    for suggestion in [
                        "info sistema",
                        "che ore sono",
                        "cerca intelligenza artificiale",
                    ] {
                        if ui.button(suggestion).clicked() {
                            self.chat_input = suggestion.into();
                        }
                    }
                }
                for message in &self.chat {
                    let (label, color) = if message.role == "user" {
                        ("TU", egui::Color32::from_rgb(112, 220, 218))
                    } else {
                        ("JARVIS", egui::Color32::from_rgb(180, 203, 222))
                    };
                    ui.horizontal_top(|ui| {
                        ui.label(egui::RichText::new(label).strong().color(color));
                        ui.label(if message.text.is_empty() && self.chat_busy {
                            self.streaming.as_str()
                        } else {
                            message.text.as_str()
                        });
                    });
                    ui.add_space(10.0);
                }
                for confirmation in self.confirmations.clone() {
                    ui.group(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 202, 120),
                            "Conferma richiesta",
                        );
                        ui.strong(confirmation["tool"].as_str().unwrap_or("Operazione"));
                        ui.label(
                            confirmation["expected_effect"]
                                .as_str()
                                .unwrap_or("Effetto non disponibile"),
                        );
                        ui.horizontal(|ui| {
                            let id = confirmation["confirmation_id"].as_str().unwrap_or_default();
                            if ui.button("Approva").clicked() {
                                self.act(
                                    ctx,
                                    format!("/api/confirmations/{id}"),
                                    Some(json!({"approve": true})),
                                );
                                self.confirmations.retain(|c| c["confirmation_id"] != id);
                            }
                            if ui.button("Rifiuta").clicked() {
                                self.act(
                                    ctx,
                                    format!("/api/confirmations/{id}"),
                                    Some(json!({"approve": false})),
                                );
                                self.confirmations.retain(|c| c["confirmation_id"] != id);
                            }
                        });
                    });
                }
            });
        ui.separator();
        ui.horizontal(|ui| {
            let input = ui.add_sized(
                [ui.available_width() - 90.0, 42.0],
                egui::TextEdit::singleline(&mut self.chat_input).hint_text("Messaggio a JARVIS…"),
            );
            let send = ui.add_enabled(
                !self.chat_busy && !self.history_loading,
                egui::Button::new("Invia"),
            );
            if send.clicked()
                || (!self.history_loading
                    && input.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                self.send_chat();
            }
        });
        if self.chat_busy {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }

    fn show_notifications(&mut self, ui: &mut egui::Ui) {
        if let Some(error) = &self.notifications_error {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Aggiornamento notifiche non riuscito: {error}"),
            );
        }
        if self.notifications.is_empty() {
            ui.label("Nessuna notifica recente.");
            return;
        }
        let notifications = self
            .notifications
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>();
        for notification in notifications {
            ui.group(|ui| {
                ui.strong(notification["title"].as_str().unwrap_or("JARVIS"));
                ui.label(notification["body"].as_str().unwrap_or(""));
                ui.small(notification["created_at"].as_str().unwrap_or(""));
                if notification["read_at"].is_null() && ui.button("Segna come letta").clicked() {
                    let id = notification["notification_id"].as_str().unwrap_or_default();
                    self.act(ui.ctx(), format!("/api/notifications/{id}/read"), None);
                }
            });
        }
    }

    fn show_data(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match self.current.as_str() {
            "dashboard" => {
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                let core_status = self
                    .data
                    .as_ref()
                    .and_then(|v| v["core"]["status"].as_str())
                    .unwrap_or("—")
                    .to_owned();
                let ai_status = self
                    .data
                    .as_ref()
                    .and_then(|v| v["ai"]["status"].as_str())
                    .unwrap_or("—")
                    .to_owned();
                let confirmations = self
                    .data
                    .as_ref()
                    .and_then(|v| v["pending_confirmations"].as_u64())
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "—".into());
                let unread = self
                    .data
                    .as_ref()
                    .and_then(|v| v["notifications_unread"].as_u64())
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "—".into());
                let version = self
                    .data
                    .as_ref()
                    .and_then(|v| v["core"]["version"].as_str())
                    .unwrap_or("—")
                    .to_owned();
                let automations = self
                    .data
                    .as_ref()
                    .and_then(|v| v["automations"].as_u64())
                    .unwrap_or(0);
                let audit_valid = self
                    .data
                    .as_ref()
                    .and_then(|v| v["audit"]["chain_valid"].as_bool())
                    .unwrap_or(false);
                ui.horizontal_wrapped(|ui| {
                    for (label, value) in [
                        ("CORE", core_status.as_str()),
                        ("AI", ai_status.as_str()),
                        ("CONFERME", confirmations.as_str()),
                        ("NOTIFICHE", unread.as_str()),
                    ] {
                        ui.group(|ui| {
                            ui.small(egui::RichText::new(label).weak());
                            ui.strong(value);
                        });
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.heading("Panoramica");
                    if ui.button("Apri chat").clicked() {
                        self.current = "chat".into();
                        self.data = None;
                    }
                });
                ui.label(format!(
                    "Versione {version} · {automations} automazioni attive · audit {}",
                    if audit_valid {
                        "verificato"
                    } else {
                        "da controllare"
                    }
                ));
                if self.loading {
                    ui.add(egui::Spinner::new());
                }
                ui.separator();
                ui.heading("Notifiche");
                self.show_notifications(ui);
                if ui.button("Segna tutte come lette").clicked() {
                    self.act(ctx, "/api/notifications/read-all".into(), None);
                }
            }
            "chat" => self.show_chat(ui, ctx),
            "models" => self.show_cloud_settings(ui, ctx),
            "updates" => self.show_updates(ui, ctx),
            "tasks" | "automations" | "memory" | "skills" | "workspace" | "connectors"
            | "devices" | "security" | "audit" | "system" => {
                ui.horizontal(|ui| {
                    ui.heading(title(&self.current));
                    if ui.button("Aggiorna").clicked() {
                        self.data = None;
                        self.refresh(ctx);
                    }
                });
                if self.loading {
                    ui.add(egui::Spinner::new());
                }
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                let items = self
                    .data
                    .as_ref()
                    .and_then(|data| data["items"].as_array())
                    .cloned();
                if let Some(items) = items {
                    if self.current == "devices" {
                        self.show_device_invite(ui, ctx);
                    }
                    for item in items {
                        self.show_item(ui, ctx, &item);
                    }
                } else {
                    if self.current == "system" && ui.button("Crea backup cifrato").clicked() {
                        self.act(ctx, "/api/backup".into(), Some(json!({})));
                    }
                    self.show_json(ui);
                }
            }
            _ => {}
        }
    }

    fn show_cloud_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Collega un provider AI");
        ui.label("Le chiavi vengono salvate nel secret store del Core e non sono mostrate dopo il salvataggio.");
        ui.add_space(8.0);

        let mut provider_changed = false;
        egui::ComboBox::from_id_salt("cloud-provider")
            .selected_text(match self.cloud_provider.as_str() {
                "nvidia_nim" => "NVIDIA NIM",
                _ => "Ollama Cloud",
            })
            .show_ui(ui, |ui| {
                provider_changed |= ui
                    .selectable_value(
                        &mut self.cloud_provider,
                        "ollama_cloud".into(),
                        "Ollama Cloud",
                    )
                    .changed();
                provider_changed |= ui
                    .selectable_value(&mut self.cloud_provider, "nvidia_nim".into(), "NVIDIA NIM")
                    .changed();
            });
        if provider_changed {
            self.cloud_api_key.clear();
            self.cloud_action_error = None;
            self.cloud_action_notice = None;
            self.cloud_remove_confirm = false;
        }

        let provider = self
            .data
            .as_ref()
            .and_then(|status| status["providers"].as_array())
            .and_then(|providers| {
                providers
                    .iter()
                    .find(|provider| provider["id"].as_str() == Some(&self.cloud_provider))
            })
            .cloned();
        let configured = provider
            .as_ref()
            .and_then(|provider| provider["configured"].as_bool())
            .unwrap_or(false);
        match provider
            .as_ref()
            .and_then(|provider| provider["configured"].as_bool())
        {
            Some(true) => {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "Chiave configurata");
            }
            Some(false) => {
                ui.colored_label(egui::Color32::YELLOW, "Chiave non configurata");
            }
            None => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label("Caricamento configurazione…");
                });
            }
        }

        if let Some(routing) = provider
            .as_ref()
            .and_then(|provider| provider["routing"].as_object())
        {
            ui.label("Modelli instradati dal Core:");
            for (route, model) in routing {
                if let Some(model) = model.as_str() {
                    ui.small(format!("{route}: {model}"));
                }
            }
        }
        ui.add_space(8.0);
        ui.label("La chiave non viene salvata in questa app né scritta nei log.");
        ui.add(
            egui::TextEdit::singleline(&mut self.cloud_api_key)
                .password(true)
                .desired_width(ui.available_width())
                .hint_text("Incolla la API key"),
        );
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.cloud_action_busy && !self.cloud_api_key.is_empty(),
                    egui::Button::new("Salva API key"),
                )
                .clicked()
            {
                self.configure_cloud(ctx, false);
            }

            if configured {
                if self.cloud_remove_confirm {
                    if ui
                        .add_enabled(
                            !self.cloud_action_busy,
                            egui::Button::new("Conferma rimozione"),
                        )
                        .clicked()
                    {
                        self.configure_cloud(ctx, true);
                    }
                    if ui.button("Annulla").clicked() {
                        self.cloud_remove_confirm = false;
                    }
                } else if ui.button("Rimuovi chiave").clicked() {
                    self.cloud_remove_confirm = true;
                }
            }
        });

        if self.cloud_action_busy {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Aggiornamento configurazione…");
            });
        }
        if let Some(error) = &self.cloud_action_error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        if let Some(notice) = &self.cloud_action_notice {
            ui.colored_label(egui::Color32::LIGHT_GREEN, notice);
        }
        ui.add_space(8.0);
        ui.small("Il salvataggio non prova la chiave. Per verificarla invia un messaggio in Chat: il testo sarà elaborato dal provider selezionato e può comportare costi.");
    }

    fn show_updates(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let data = self.data.clone();
        let state = data.as_ref();
        let mut enabled = state
            .and_then(|state| state["enabled"].as_bool())
            .unwrap_or(true);
        let mut channel = state
            .and_then(|state| state["channel"].as_str())
            .unwrap_or("stable")
            .to_owned();
        let token_configured = state
            .and_then(|state| state["github_token_configured"].as_bool())
            .unwrap_or(false);
        let latest = state
            .and_then(|state| state.get("latest"))
            .filter(|v| !v.is_null());

        ui.heading("Aggiornamenti JARVIS");
        ui.label("Controllo automatico ogni ora; download, verifica della firma e installazione restano manuali.");
        ui.add_space(8.0);
        ui.checkbox(&mut enabled, "Controlla automaticamente le nuove release");
        egui::ComboBox::from_id_salt("update-channel")
            .selected_text(if channel == "beta" { "Beta" } else { "Stable" })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut channel, "stable".into(), "Stable");
                ui.selectable_value(&mut channel, "beta".into(), "Beta");
            });
        ui.label("Stable ignora le prerelease; Beta include anche le versioni di prova.");

        if token_configured {
            ui.colored_label(
                egui::Color32::LIGHT_GREEN,
                "Token GitHub configurato nel secret store cifrato del Core.",
            );
            if self.update_remove_confirm {
                ui.horizontal(|ui| {
                    if ui.button("Conferma rimozione token").clicked() {
                        self.act(
                            ctx,
                            "/api/updates".into(),
                            Some(json!({
                                "enabled": enabled,
                                "channel": channel,
                                "remove_github_token": true
                            })),
                        );
                        self.update_remove_confirm = false;
                    }
                    if ui.button("Annulla").clicked() {
                        self.update_remove_confirm = false;
                    }
                });
            } else if ui.button("Rimuovi token GitHub").clicked() {
                self.update_remove_confirm = true;
            }
        } else {
            ui.label("Repository pubblica: lascia il token vuoto. Repository privata: inserisci un token fine-grained con accesso Contents: read solo a JARVIS.");
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.github_update_token)
                .password(true)
                .desired_width(ui.available_width())
                .hint_text(if token_configured {
                    "Token attuale conservato; inserisci un nuovo token solo per sostituirlo"
                } else {
                    "Token GitHub opzionale per repository privata"
                }),
        );
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.loading, egui::Button::new("Salva impostazioni"))
                .clicked()
            {
                let mut body = json!({"enabled": enabled, "channel": channel});
                let token = std::mem::take(&mut self.github_update_token);
                if !token.is_empty() {
                    body["github_token"] = json!(token);
                }
                self.act(ctx, "/api/updates".into(), Some(body));
                self.update_remove_confirm = false;
            }
            if ui
                .add_enabled(!self.loading, egui::Button::new("Controlla ora"))
                .clicked()
            {
                self.act(ctx, "/api/updates/check".into(), None);
            }
        });

        if self.loading {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Controllo o salvataggio in corso…");
            });
        }
        if let Some(error) = state.and_then(|state| state["error"].as_str()) {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Ultimo controllo fallito: {error}"),
            );
        }
        if let Some(checked_at) = state.and_then(|state| state["checked_at"].as_str()) {
            ui.small(format!("Ultimo controllo: {checked_at}"));
        }
        if let Some(latest) = latest {
            let tag = latest["tag"].as_str().unwrap_or("versione sconosciuta");
            ui.separator();
            if state.is_some_and(|state| state["update_available"] == true) {
                ui.colored_label(
                    egui::Color32::LIGHT_GREEN,
                    format!("Nuova versione disponibile: {tag}"),
                );
            } else {
                ui.label(format!("Versione più recente: {tag}"));
            }
            let release_url = latest["url"].as_str().unwrap_or_default();
            let expected_prefix = "https://github.com/xX-VyrnCore-Xx/JARVIS/releases/tag/";
            if release_url.starts_with(expected_prefix)
                && ui.link("Apri release e scarica").clicked()
            {
                ctx.open_url(egui::OpenUrl::new_tab(release_url));
            }
        }
        ui.small("Per i repository privati il Core usa il token solo per leggere le release GitHub; la chiave non viene restituita dall'API né inclusa nelle notifiche.");
    }

    fn show_device_invite(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.group(|ui| {
            ui.heading("Associa un dispositivo");
            ui.checkbox(
                &mut self.invite_confirm,
                "Consenti le conferme (oltre alla chat)",
            );
            if ui
                .add_enabled(
                    !self.invite_creating,
                    egui::Button::new("Crea codice monouso"),
                )
                .clicked()
            {
                self.create_device_invite(ctx);
            }
            if let Some(invite) = &self.device_invite {
                ui.label(format!(
                    "Codice: {}",
                    invite["code"].as_str().unwrap_or("—")
                ));
                ui.label(format!(
                    "Scade: {}",
                    invite["expires_at"].as_str().unwrap_or("—")
                ));
                if ui.button("Annulla invito").clicked() {
                    let code = invite["code"].as_str().unwrap_or_default();
                    self.act(
                        ctx,
                        "/api/devices/pairing/cancel".into(),
                        Some(json!({"code": code})),
                    );
                    self.device_invite = None;
                }
            }
        });
    }

    fn show_item(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, item: &Value) {
        let mut memory_save = None;
        ui.group(|ui| {
            let title = item["name"]
                .as_str()
                .or_else(|| item["title"].as_str())
                .or_else(|| item["content"].as_str())
                .or_else(|| item["task_id"].as_str())
                .unwrap_or("Elemento");
            ui.strong(title);
            let state = item["state"]
                .as_str()
                .or_else(|| item["status"].as_str())
                .or_else(|| item["kind"].as_str());
            if let Some(state) = state {
                ui.small(state);
            }
            if let Some(body) = item["body"].as_str() {
                ui.label(body);
            }
            if let Some(objective) = item["objective"].as_str() {
                ui.label(objective);
            }
            ui.horizontal_wrapped(|ui| {
                if self.current == "automations" {
                    if let Some(id) = item["workflow_id"].as_str().or_else(|| item["id"].as_str()) {
                        if ui.button("Esegui ora").clicked() {
                            self.act(ctx, format!("/api/workflows/{id}/run"), Some(json!({})));
                        }
                        if ui
                            .button(if item["enabled"].as_bool().unwrap_or(false) {
                                "Disattiva"
                            } else {
                                "Attiva"
                            })
                            .clicked()
                        {
                            self.act(
                                ctx,
                                format!("/api/workflows/{id}/enabled"),
                                Some(
                                    json!({"enabled": !item["enabled"].as_bool().unwrap_or(false)}),
                                ),
                            );
                        }
                    }
                }
                if self.current == "memory" {
                    if let Some(id) = item["memory_id"].as_str().or_else(|| item["id"].as_str()) {
                        let initial = item["text"]
                            .as_str()
                            .or_else(|| item["content"].as_str())
                            .unwrap_or_default()
                            .to_string();
                        let edit = self.memory_edits.entry(id.to_string()).or_insert(initial);
                        ui.add(egui::TextEdit::singleline(edit).desired_width(f32::INFINITY));
                        if ui.button("Salva correzione").clicked() {
                            memory_save = Some((id.to_string(), edit.clone()));
                        }
                        if ui.button("Dimentica").clicked() {
                            self.pending_delete = Some((id.into(), title.into()));
                        }
                    }
                }
                if self.current == "devices" && item["revoked_at"].is_null() {
                    if let Some(id) = item["device_id"].as_str() {
                        if ui.button("Revoca").clicked() {
                            self.pending_delete = Some((format!("device:{id}"), title.into()));
                        }
                    }
                }
            });
            egui::CollapsingHeader::new("Dettagli")
                .id_salt(item["id"].as_str().unwrap_or(title))
                .show(ui, |ui| {
                    ui.monospace(serde_json::to_string_pretty(item).unwrap_or_default());
                });
        });
        if let Some((id, text)) = memory_save {
            self.mutate(
                ctx,
                Mutation::Patch,
                format!("/api/memory/{id}"),
                Some(json!({"text": text})),
            );
        }
        if self.pending_delete.is_some() {
            let pending = self.pending_delete.clone().unwrap();
            egui::Window::new("Conferma operazione")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!("Vuoi procedere per «{}»?", pending.1));
                    ui.horizontal(|ui| {
                        if ui.button("Annulla").clicked() {
                            self.pending_delete = None;
                        }
                        if ui.button("Conferma").clicked() {
                            if let Some(id) = pending.0.strip_prefix("device:") {
                                self.act(ctx, format!("/api/devices/{id}/revoke"), Some(json!({})));
                            } else {
                                self.mutate(
                                    ctx,
                                    Mutation::Delete,
                                    format!("/api/memory/{}", pending.0),
                                    None,
                                );
                            }
                            self.pending_delete = None;
                        }
                    });
                });
        }
    }

    fn show_json(&self, ui: &mut egui::Ui) {
        let Some(data) = &self.data else {
            if !self.loading {
                ui.label("Dati non disponibili.");
            }
            return;
        };
        let pretty = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".into());
        ui.monospace(pretty);
    }

    fn show_pairing(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading("Collega il Core JARVIS");
            ui.label(
                "Inserisci l’origine HTTPS del Core e il codice temporaneo creato in Dispositivi.",
            );
            ui.add_space(16.0);
            ui.add_sized(
                [ui.available_width().min(460.0), 40.0],
                egui::TextEdit::singleline(&mut self.remote_url)
                    .hint_text("https://jarvis.example.com"),
            );
            ui.add_sized(
                [ui.available_width().min(460.0), 40.0],
                egui::TextEdit::singleline(&mut self.pair_code).hint_text("Codice monouso"),
            );
            if let Some(error) = &self.pair_error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            if ui
                .add_enabled(!self.pairing, egui::Button::new("Associa dispositivo"))
                .clicked()
            {
                self.pair_remote(ctx);
            }
        });
    }

    fn pair_remote(&mut self, ctx: &egui::Context) {
        let base = self.remote_url.trim().to_string();
        let code = self.pair_code.trim().to_string();
        let client = match ApiClient::unauthenticated(&base) {
            Ok(client) if is_https_url(&base) => client,
            Ok(_) => {
                self.pair_error =
                    Some("Per associare un dispositivo serve un indirizzo HTTPS valido.".into());
                return;
            }
            Err(error) => {
                self.pair_error = Some(error);
                return;
            }
        };
        self.pairing = true;
        let tx = self.tx.clone();
        let data_dir = data_dir();
        std::thread::spawn(move || {
            let result = client.pair(&code).and_then(|response| {
                let token = response["token"]
                    .as_str()
                    .ok_or("Token del dispositivo assente")?;
                std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
                std::fs::write(data_dir.join("remote-core-url.txt"), &base)
                    .map_err(|e| e.to_string())?;
                let token_path = data_dir.join("remote-device-token");
                std::fs::write(&token_path, token).map_err(|e| e.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600))
                        .map_err(|e| e.to_string())?;
                }
                ApiClient::device(&base, token.to_string()).map(|client| Connection {
                    client,
                    core: None,
                    remote: true,
                })
            });
            let _ = tx.send(Event::Connected(Box::new(result)));
        });
        self.pair_error = None;
        ctx.request_repaint();
    }
}

impl eframe::App for JarvisApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        self.receive_events(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive_events(&ctx);
        if self.current != "models" {
            self.cloud_api_key.clear();
            self.cloud_action_error = None;
            self.cloud_action_notice = None;
            self.cloud_remove_confirm = false;
        }
        if self.current != "updates" {
            self.github_update_token.clear();
            self.update_remove_confirm = false;
        }
        self.focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if self.client.is_none() {
            if self.error.as_deref() == Some("remote-pairing") {
                egui::CentralPanel::default().show(ui, |ui| self.show_pairing(ui, &ctx));
            } else {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(120.0);
                        ui.heading("JARVIS");
                        ui.add(egui::Spinner::new());
                        ui.label(self.error.as_deref().unwrap_or("Avvio del Core…"));
                    });
                });
            }
            ctx.request_repaint_after(Duration::from_millis(250));
            return;
        }

        egui::Panel::top("topbar").exact_size(58.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.heading(
                    egui::RichText::new("JARVIS").color(egui::Color32::from_rgb(118, 236, 229)),
                );
                ui.separator();
                ui.label(title(&self.current));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(if self.remote {
                        "DISPOSITIVO ASSOCIATO"
                    } else {
                        "CORE LOCALE"
                    });
                    if ui
                        .button(format!(
                            "🔔 {}",
                            self.notifications
                                .iter()
                                .filter(|n| n["read_at"].is_null())
                                .count()
                        ))
                        .clicked()
                    {
                        self.current = "dashboard".into();
                        self.data = None;
                        self.refresh(&ctx);
                    }
                });
            });
        });

        let compact = ctx.viewport_rect().width() < 720.0;
        egui::Panel::left("navigation")
            .resizable(false)
            .exact_size(if compact { 58.0 } else { 190.0 })
            .show(ui, |ui| {
                let footer_height = 34.0;
                let navigation_height = (ui.available_height() - footer_height).max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), navigation_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.add_space(12.0);
                        egui::ScrollArea::vertical()
                            .id_salt("navigation-list")
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.navigation(ui, compact, &ctx));
                    },
                );
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.separator();
                    ui.label(if self.remote {
                        "Core remoto"
                    } else {
                        "Core attivo"
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(12.0);
            if self.current == "chat" {
                self.show_chat(ui, &ctx);
            } else {
                egui::ScrollArea::vertical()
                    .id_salt(("main", &self.current))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.show_data(ui, &ctx);
                        ui.add_space(24.0);
                    });
            }
        });

        if self.current != "chat" && self.last_load.elapsed() > Duration::from_secs(15) {
            self.refresh(&ctx);
        }
        if self.core.as_mut().is_some_and(|core| !core.is_alive()) {
            self.error = Some("Il Core si è arrestato. Riavvia JARVIS per riconnetterti.".into());
        }
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

fn endpoint(page: &str) -> &'static str {
    match page {
        "dashboard" => "/api/health",
        "tasks" => "/api/tasks?limit=50",
        "automations" => "/api/workflows",
        "memory" => "/api/memory",
        "skills" => "/api/skills",
        "workspace" => "/api/workspace",
        "connectors" => "/api/connectors",
        "devices" => "/api/devices",
        "security" => "/api/security",
        "audit" => "/api/audit",
        "system" => "/api/setup",
        "models" => "/api/cloud",
        "updates" => "/api/updates",
        _ => "/api/health",
    }
}

fn title(page: &str) -> &'static str {
    NAV.iter()
        .find(|(id, _, _)| *id == page)
        .map(|(_, label, _)| *label)
        .unwrap_or("JARVIS")
}

fn parse_history(history: &Value) -> Vec<ChatMessage> {
    history
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let role = item["role"].as_str()?;
            let text = item["content"].as_str()?;
            matches!(role, "user" | "assistant").then(|| ChatMessage {
                role: if role == "user" { "user" } else { "assistant" },
                text: text.to_owned(),
            })
        })
        .collect()
}

fn connect_core() -> Result<Connection, String> {
    let data_dir = data_dir();
    let remote = std::env::var("JARVIS_REMOTE_URL")
        .ok()
        .or_else(|| std::fs::read_to_string(data_dir.join("remote-core-url.txt")).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(remote) = remote {
        let mut url = url::Url::parse(&remote).map_err(|_| "Indirizzo del Core non valido")?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err("Usa l’origine HTTPS del Core, senza percorso né credenziali".into());
        }
        url.set_path("/");
        let token_path = data_dir.join("remote-device-token");
        if let Ok(token) = std::fs::read_to_string(token_path) {
            if !token.trim().is_empty() {
                return ApiClient::device(url.as_str(), token.trim().to_string()).map(|client| {
                    Connection {
                        client,
                        core: None,
                        remote: true,
                    }
                });
            }
        }
        let _ = ApiClient::unauthenticated(url.as_str())?;
        return Err("remote-pairing".into());
    }

    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let resources = resources_dir();
    let bundled = resources
        .as_deref()
        .filter(|path| path.join("ai/jarvis_ai/worker.py").is_file())
        .and_then(|resources| {
            let core = resources.join(if cfg!(windows) {
                "jarvis-core.exe"
            } else {
                "jarvis-core"
            });
            let python = if cfg!(windows) {
                resources.join("python/python.exe")
            } else {
                resources.join("python/bin/python3")
            };
            core.is_file().then(|| LaunchConfig {
                home: resources.to_path_buf(),
                core_bin: core,
                data_dir: data_dir.clone(),
                python: python.to_string_lossy().into_owned(),
                env: vec![("PYTHONDONTWRITEBYTECODE".into(), "1".into())],
                startup_timeout: Duration::from_secs(90),
            })
        });
    let config = match bundled {
        Some(config) => config,
        None => {
            let home = std::env::var_os("JARVIS_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
            LaunchConfig::discover(&home, &data_dir).map_err(|e| e.to_string())?
        }
    };
    let core = launch(&config).map_err(|e| e.to_string())?;
    let client = ApiClient::owner(&core.url, core.token.clone())?;
    Ok(Connection {
        client,
        core: Some(core),
        remote: false,
    })
}

fn configured_remote_url() -> String {
    std::env::var("JARVIS_REMOTE_URL")
        .ok()
        .or_else(|| std::fs::read_to_string(data_dir().join("remote-core-url.txt")).ok())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("JARVIS_DATA_DIR") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "windows")]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join("JARVIS");
    }
    #[cfg(target_os = "macos")]
    if let Some(path) = std::env::var_os("HOME") {
        return PathBuf::from(path).join("Library/Application Support/JARVIS");
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(path).join("jarvis");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".local/share/jarvis")
}

fn resources_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let parent = exe.parent()?;
    let windows_resources = parent.join("resources");
    if windows_resources.is_dir() {
        return Some(windows_resources);
    }
    let mac_resources = parent.parent()?.join("Resources");
    mac_resources.is_dir().then_some(mac_resources)
}

#[allow(non_snake_case)]
fn is_https_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some())
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn show_notification(item: &Value) {
    let title = item["title"].as_str().unwrap_or("JARVIS");
    let body = item["body"].as_str().unwrap_or("");
    if let Err(error) = notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show()
    {
        eprintln!("Notifica desktop non visualizzata: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_navigation_item_has_an_api_source() {
        for (id, _, _) in NAV {
            assert!(!endpoint(id).is_empty(), "missing endpoint for {id}");
        }
    }

    #[test]
    fn cloud_provider_settings_use_the_owner_cloud_endpoint() {
        assert_eq!(endpoint("models"), "/api/cloud");
        assert!(NAV
            .iter()
            .any(|(id, _, owner_only)| *id == "models" && *owner_only));
        assert_eq!(endpoint("updates"), "/api/updates");
        assert!(NAV
            .iter()
            .any(|(id, _, owner_only)| *id == "updates" && *owner_only));
    }

    #[test]
    fn remote_associated_devices_only_see_chat_and_tasks() {
        let visible: Vec<_> = NAV
            .iter()
            .filter(|(_, _, owner_only)| !owner_only)
            .map(|(id, _, _)| *id)
            .collect();
        assert_eq!(visible, ["chat", "tasks"]);
    }

    #[test]
    fn restores_only_text_messages_from_conversation_history() {
        let history = json!({"items": [
            {"role": "user", "content": "Ciao"},
            {"role": "assistant", "content": "Buongiorno"},
            {"role": "tool", "content": "dettaglio interno"},
            {"role": "user", "content": 7}
        ]});
        let messages = parse_history(&history);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].text, "Buongiorno");
    }
}
