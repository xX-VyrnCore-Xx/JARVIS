use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::{json, Value};

use crate::client::ApiClient;
use jarvis_launcher::{launch, LaunchConfig, RunningCore};

const BACKGROUND: egui::Color32 = egui::Color32::from_rgb(5, 13, 24);
const SURFACE: egui::Color32 = egui::Color32::from_rgb(10, 24, 39);
const RAISED: egui::Color32 = egui::Color32::from_rgb(16, 36, 55);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(83, 215, 241);
const SELECTED: egui::Color32 = egui::Color32::from_rgb(13, 69, 91);
const POSITIVE: egui::Color32 = egui::Color32::from_rgb(105, 220, 174);
const CAUTION: egui::Color32 = egui::Color32::from_rgb(245, 190, 96);
const NEGATIVE: egui::Color32 = egui::Color32::from_rgb(255, 125, 135);

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
    BackendsUpdated(Result<Vec<CoreBackend>, String>),
    History(Result<Value, String>),
    ActionDone(Result<Value, String>),
    ConnectorOAuth(Result<Value, String>),
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

#[derive(Clone, Debug)]
struct CoreBackend {
    origin: String,
    token: String,
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
    startup_icon: egui::TextureHandle,
    client: Option<Arc<ApiClient>>,
    core: Option<RunningCore>,
    remote: bool,
    current: String,
    data: Option<Value>,
    error: Option<String>,
    reduce_motion: bool,
    loading: bool,
    last_load: Instant,
    chat_input: String,
    chat: Vec<ChatMessage>,
    memory_search: String,
    history_loading: bool,
    streaming: String,
    chat_busy: bool,
    chat_cloud: bool,
    chat_cloud_context: bool,
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
    backend_url: String,
    backend_pair_code: String,
    backend_pairing: bool,
    backend_pair_error: Option<String>,
    backends: Vec<CoreBackend>,
    backend_config_error: Option<String>,
    pending_backend_removal: Option<String>,
    pending_delete: Option<(String, String)>,
    device_invite: Option<Value>,
    pairing_url: String,
    setup_stt: String,
    setup_tts: String,
    connector_template: String,
    connector_client_id: String,
    connector_credential: String,
    connector_oauth_notice: Option<String>,
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
        let icon = application_icon();
        let startup_icon = cc.egui_ctx.load_texture(
            "jarvis-startup-icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            ),
            egui::TextureOptions::LINEAR,
        );
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.window_fill = SURFACE;
        style.visuals.panel_fill = BACKGROUND;
        style.visuals.widgets.inactive.bg_fill = RAISED;
        style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(10);
        style.visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(37, 67, 87));
        style.visuals.widgets.active.bg_fill = SELECTED;
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(10);
        style.visuals.widgets.active.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(48, 158, 187));
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(24, 67, 86);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(10);
        style.visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 122, 141));
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(24, 76, 91);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
        style.spacing.item_spacing = egui::vec2(12.0, 10.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.interact_size = egui::vec2(40.0, 38.0);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);

        let (tx, rx) = mpsc::channel();
        spawn_connection(tx.clone());
        let (backends, backend_config_error) =
            match load_core_backends(&data_dir().join("remote-backends.json")) {
                Ok(backends) => (backends, None),
                Err(error) => (Vec::new(), Some(error)),
            };

        Self {
            tx,
            rx,
            startup_icon,
            client: None,
            core: None,
            remote: false,
            current: "dashboard".into(),
            data: None,
            error: None,
            reduce_motion: false,
            loading: false,
            last_load: Instant::now() - Duration::from_secs(60),
            chat_input: String::new(),
            chat: Vec::new(),
            memory_search: String::new(),
            history_loading: false,
            streaming: String::new(),
            chat_busy: false,
            chat_cloud: false,
            chat_cloud_context: false,
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
            backend_url: String::new(),
            backend_pair_code: String::new(),
            backend_pairing: false,
            backend_pair_error: None,
            backends,
            backend_config_error,
            pending_backend_removal: None,
            pending_delete: None,
            device_invite: None,
            pairing_url: String::new(),
            setup_stt: "small".into(),
            setup_tts: "it_IT-paola-medium".into(),
            connector_template: String::new(),
            connector_client_id: String::new(),
            connector_credential: String::new(),
            connector_oauth_notice: None,
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
                Event::BackendsUpdated(result) => {
                    self.backend_pairing = false;
                    match result {
                        Ok(backends) => {
                            self.backends = backends;
                            self.backend_config_error = None;
                            self.backend_pair_error = None;
                            self.backend_url.clear();
                            self.backend_pair_code.clear();
                            self.data = None;
                            self.last_load = Instant::now() - Duration::from_secs(60);
                            self.refresh(ctx);
                        }
                        Err(error) => self.backend_pair_error = Some(error),
                    }
                    ctx.request_repaint();
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
                Event::ConnectorOAuth(result) => {
                    self.loading = false;
                    match result {
                        Ok(data) => {
                            if let Some(url) = data["authorize_url"]
                                .as_str()
                                .filter(|url| safe_oauth_authorize_url(url))
                            {
                                ctx.open_url(egui::OpenUrl::new_tab(url));
                                self.error = None;
                                self.connector_oauth_notice = Some(
                                    "Completa l'autorizzazione nel browser, poi aggiorna i connettori per verificarne lo stato.".into(),
                                );
                            } else {
                                self.connector_oauth_notice = None;
                                self.error = Some(
                                    "Il Core non ha restituito un URL OAuth HTTPS valido.".into(),
                                );
                            }
                        }
                        Err(error) => {
                            self.connector_oauth_notice = None;
                            self.error = Some(error);
                        }
                    }
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
                    if let Some(confirmation) =
                        result.get("confirmation").filter(|value| value.is_object())
                    {
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
                    }
                    self.notifications_primed = true;
                    self.known_notifications = items
                        .iter()
                        .filter_map(|item| item["notification_id"].as_str().map(str::to_owned))
                        .collect();
                    self.notifications = items;
                    ctx.request_repaint();
                }
                Event::Notifications(Err(error)) => self.notifications_error = Some(error),
            }
        }
    }

    fn retry_connection(&mut self, ctx: &egui::Context) {
        self.error = None;
        spawn_connection(self.tx.clone());
        ctx.request_repaint();
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
        let backends = self.backends.clone();
        let primary_name = if self.remote {
            backend_display_name(&self.remote_url)
        } else {
            "Core locale".into()
        };
        let primary_origin = if self.remote {
            normalize_core_origin(&self.remote_url).unwrap_or_else(|_| self.remote_url.clone())
        } else {
            "Core locale".into()
        };
        let tx = self.tx.clone();
        let path = if page == "memory" {
            memory_endpoint(&self.memory_search)
        } else {
            endpoint(&page).to_owned()
        };
        std::thread::spawn(move || {
            let result = if page == "dashboard" {
                load_dashboard(&client, &primary_name, &primary_origin, &backends)
            } else {
                client.get(&path).and_then(|mut data| {
                    if page == "connectors" {
                        data["catalog"] =
                            client.get("/api/connectors/templates")?["templates"].clone();
                    }
                    Ok(data)
                })
            };
            let _ = tx.send(Event::Loaded(page, result));
        });
        ctx.request_repaint();
    }

    fn add_core_backend(&mut self, ctx: &egui::Context) {
        if self.backend_config_error.is_some() {
            self.backend_pair_error =
                Some("Correggi prima la configurazione dei Core associati.".into());
            return;
        }
        let origin = match normalize_core_origin(&self.backend_url) {
            Ok(origin) => origin,
            Err(error) => {
                self.backend_pair_error = Some(error);
                return;
            }
        };
        if !is_https_url(&origin) {
            self.backend_pair_error =
                Some("Per associare un Core aggiuntivo serve un indirizzo HTTPS valido.".into());
            return;
        }
        if self.backends.iter().any(|backend| backend.origin == origin)
            || (self.remote
                && normalize_core_origin(&self.remote_url).is_ok_and(|primary| primary == origin))
        {
            self.backend_pair_error = Some("Questo Core è già collegato.".into());
            return;
        }
        let code = self.backend_pair_code.trim().to_owned();
        if code.is_empty() {
            self.backend_pair_error = Some("Inserisci il codice monouso del Core.".into());
            return;
        }

        self.backend_pairing = true;
        self.backend_pair_error = None;
        let tx = self.tx.clone();
        let path = data_dir().join("remote-backends.json");
        std::thread::spawn(move || {
            let result = ApiClient::unauthenticated(&origin)
                .and_then(|client| client.pair(&code))
                .and_then(|response| {
                    response["token"]
                        .as_str()
                        .filter(|token| !token.is_empty())
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            "Il Core non ha restituito le credenziali del dispositivo".into()
                        })
                })
                .and_then(|token| {
                    let mut backends = load_core_backends(&path)?;
                    if backends.iter().any(|backend| backend.origin == origin) {
                        return Err("Questo Core è già collegato.".into());
                    }
                    backends.push(CoreBackend { origin, token });
                    save_core_backends(&path, &backends)?;
                    Ok(backends)
                });
            let _ = tx.send(Event::BackendsUpdated(result));
        });
        ctx.request_repaint();
    }

    fn forget_core_backend(&mut self, origin: &str, ctx: &egui::Context) {
        let backends: Vec<_> = self
            .backends
            .iter()
            .filter(|backend| backend.origin != origin)
            .cloned()
            .collect();
        match save_core_backends(&data_dir().join("remote-backends.json"), &backends) {
            Ok(()) => {
                self.backends = backends;
                self.backend_config_error = None;
                self.pending_backend_removal = None;
                self.data = None;
                self.last_load = Instant::now() - Duration::from_secs(60);
                self.refresh(ctx);
            }
            Err(error) => self.backend_config_error = Some(error),
        }
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
        let cloud = self.chat_cloud && !self.remote;
        let use_context = self.chat_cloud_context;
        std::thread::spawn(move || {
            let result = if cloud {
                client.post(
                    "/api/cloud/chat",
                    Some(&json!({
                        "provider": "auto", "message": message, "use_context": use_context,
                    })),
                )
            } else {
                client.chat_stream(&message, |delta| {
                    let _ = tx.send(Event::ChatDelta(delta.to_string()));
                })
            };
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
            if self.remote && owner_only {
                continue;
            }
            let selected = self.current == id;
            let button = egui::Button::new(egui::RichText::new(label).color(if selected {
                egui::Color32::from_rgb(177, 245, 246)
            } else {
                egui::Color32::from_rgb(180, 195, 207)
            }))
            .fill(if selected {
                SELECTED
            } else {
                egui::Color32::TRANSPARENT
            })
            .corner_radius(egui::CornerRadius::same(10))
            .stroke(if selected {
                egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 105, 121))
            } else {
                egui::Stroke::NONE
            });
            let width = if compact { 122.0 } else { ui.available_width() };
            let response = ui.add_sized([width, 42.0], button);
            if selected {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        response.rect.min + egui::vec2(3.0, 8.0),
                        egui::vec2(2.0, response.rect.height() - 16.0),
                    ),
                    egui::CornerRadius::same(1),
                    ACCENT,
                );
            }
            if response.clicked() {
                self.connector_credential.clear();
                self.current = id.into();
                self.data = None;
                self.refresh(ctx);
            }
        }
    }

    fn show_chat(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.remote {
            ui.add_enabled_ui(!self.chat_busy, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.chat_cloud, false, "Assistente locale");
                    ui.selectable_value(&mut self.chat_cloud, true, "AI cloud");
                    if self.chat_cloud {
                        ui.checkbox(&mut self.chat_cloud_context, "Includi turni cloud recenti")
                            .on_hover_text("Condivide i turni precedenti dello stesso provider con la richiesta corrente.");
                    }
                });
            });
        }
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
            .auto_shrink([false, false])
            .max_height((ui.available_height() - 75.0).max(0.0))
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
                    egui::Frame::new()
                        .fill(if message.role == "user" {
                            SELECTED
                        } else {
                            RAISED
                        })
                        .corner_radius(12)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(egui::RichText::new(label).small().strong().color(color));
                            ui.add(
                                egui::Label::new(if message.text.is_empty() && self.chat_busy {
                                    self.streaming.as_str()
                                } else {
                                    message.text.as_str()
                                })
                                .wrap()
                                .selectable(true),
                            );
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
                [(ui.available_width() - 90.0).max(0.0), 42.0],
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
            let severity = notification["severity"].as_str().unwrap_or("info");
            let category = notification["category"].as_str().unwrap_or("system");
            let unread = notification["read_at"].is_null();
            let color = match severity {
                "error" => egui::Color32::LIGHT_RED,
                "warning" => egui::Color32::from_rgb(255, 197, 108),
                "success" => egui::Color32::from_rgb(80, 220, 160),
                _ => ACCENT,
            };
            egui::Frame::new()
                .fill(if unread { SELECTED } else { RAISED })
                .corner_radius(12)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(if unread { "●" } else { "○" }).color(color));
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(notification["title"].as_str().unwrap_or("JARVIS"));
                                ui.small(
                                    egui::RichText::new(format!("{category} · {severity}"))
                                        .color(color),
                                );
                            });
                            if let Some(body) = notification["body"].as_str() {
                                ui.label(body);
                            }
                            if let Some(created_at) = notification["created_at"].as_str() {
                                ui.small(egui::RichText::new(created_at).weak());
                            }
                        });
                    });
                    ui.horizontal_wrapped(|ui| {
                        if let Some(view) = notification["link"]["view"].as_str() {
                            let allowed = NAV.iter().any(|(id, _, owner_only)| {
                                *id == view && (!self.remote || !*owner_only)
                            });
                            if allowed && ui.button("Apri").clicked() {
                                self.current = view.to_owned();
                                self.data = None;
                                self.refresh(ui.ctx());
                            }
                        }
                        if unread && ui.button("Segna come letta").clicked() {
                            let id = notification["notification_id"].as_str().unwrap_or_default();
                            self.act(ui.ctx(), format!("/api/notifications/{id}/read"), None);
                        }
                    });
                });
        }
    }

    fn show_data(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match self.current.as_str() {
            "dashboard" => {
                self.show_dashboard(ui, ctx);
            }
            "chat" => self.show_chat(ui, ctx),
            "models" => self.show_cloud_settings(ui, ctx),
            "updates" => self.show_updates(ui, ctx),
            "system" => self.show_setup(ui, ctx),
            "connectors" => self.show_connectors(ui, ctx),
            "memory" => self.show_memory(ui, ctx),
            "tasks" | "automations" | "skills" | "workspace" | "devices" | "security" | "audit" => {
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
                    self.show_json(ui);
                }
            }
            _ => {}
        }
    }

    fn show_dashboard(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }

        let core_status = self
            .data
            .as_ref()
            .and_then(|data| {
                data["aggregate"]["core_label"]
                    .as_str()
                    .or_else(|| data["core"]["status"].as_str())
            })
            .unwrap_or("—")
            .to_owned();
        let ai_status = self
            .data
            .as_ref()
            .and_then(|data| {
                data["aggregate"]["ai_label"]
                    .as_str()
                    .or_else(|| data["ai"]["status"].as_str())
            })
            .unwrap_or("—")
            .to_owned();
        let confirmations = self
            .data
            .as_ref()
            .and_then(|data| data["pending_confirmations"].as_u64())
            .map(|count| count.to_string())
            .unwrap_or_else(|| "—".into());
        let unread = self
            .data
            .as_ref()
            .and_then(|data| data["notifications_unread"].as_u64())
            .map(|count| count.to_string())
            .unwrap_or_else(|| "—".into());
        let version = self
            .data
            .as_ref()
            .and_then(|data| data["core"]["version"].as_str())
            .unwrap_or("—")
            .to_owned();
        let automations = self
            .data
            .as_ref()
            .and_then(|data| data["automations"].as_u64())
            .map(|count| count.to_string())
            .unwrap_or_else(|| "—".into());
        let audit_status = self.data.as_ref().map_or("—", |data| {
            if data["aggregate"]["all_online"] == false {
                "Parziale"
            } else {
                match data["audit"]["chain_valid"].as_bool() {
                    Some(true) => "Verificato",
                    Some(false) => "Da controllare",
                    None => "—",
                }
            }
        });
        let core_color = self.data.as_ref().map_or(ACCENT, dashboard_core_color);
        let ai_color = self
            .data
            .as_ref()
            .map_or(ACCENT, |data| dashboard_ai_color(data, &ai_status));

        egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_rgb(30, 112, 145),
            ))
            .corner_radius(22)
            .inner_margin(18)
            .show(ui, |ui| {
                let width = ui.available_width();
                ui.set_min_width(width);

                let time = ctx.input(|input| input.time) as f32;
                let pulse = if self.reduce_motion {
                    0.0
                } else {
                    (time * 1.1).sin() * 0.5 + 0.5
                };
                let rect = ui.max_rect();
                let painter = ui.painter().with_clip_rect(rect);
                let orbit_x = if width >= 900.0 {
                    rect.left() + rect.width() * 0.59
                } else {
                    rect.right() - rect.width() * 0.22
                };
                let orbit = egui::pos2(orbit_x, rect.center().y);
                painter.circle_filled(
                    orbit,
                    96.0 + pulse * 8.0,
                    egui::Color32::from_rgba_unmultiplied(25, 161, 210, 8),
                );
                painter.circle_stroke(
                    orbit,
                    108.0 + pulse * 4.0,
                    egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(64, 194, 225, 42)),
                );
                painter.circle_stroke(
                    orbit,
                    122.0,
                    egui::Stroke::new(0.7, egui::Color32::from_rgba_unmultiplied(64, 194, 225, 22)),
                );

                if width < 570.0 {
                    self.dashboard_intro(ui, ctx, &core_status, &ai_status, core_color, ai_color);
                    ui.add_space(6.0);
                    ui.vertical_centered(|ui| show_avatar(ui, ctx, self.reduce_motion));
                    ui.add_space(4.0);
                    self.dashboard_actions(ui, ctx);
                } else if width >= 900.0 {
                    ui.set_height(228.0);
                    let intro_width = (width * 0.49).clamp(280.0, width - 424.0);
                    let avatar_width = 220.0;
                    let actions_width = (width - intro_width - avatar_width - 28.0).max(170.0);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(intro_width, 220.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                self.dashboard_intro(
                                    ui,
                                    ctx,
                                    &core_status,
                                    &ai_status,
                                    core_color,
                                    ai_color,
                                )
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(avatar_width, 220.0),
                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| show_avatar(ui, ctx, self.reduce_motion),
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(actions_width, 220.0),
                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| self.dashboard_actions(ui, ctx),
                        );
                    });
                } else {
                    ui.set_height(240.0);
                    let intro_width = width * 0.62;
                    let avatar_width = (width - intro_width - 12.0).max(132.0);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(intro_width, 220.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                self.dashboard_intro(
                                    ui,
                                    ctx,
                                    &core_status,
                                    &ai_status,
                                    core_color,
                                    ai_color,
                                )
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(avatar_width, 220.0),
                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| show_avatar(ui, ctx, self.reduce_motion),
                        );
                    });
                    ui.add_space(4.0);
                    self.dashboard_actions(ui, ctx);
                }
            });

        ui.add_space(16.0);
        ui.horizontal_wrapped(|ui| {
            ui.heading("Panoramica");
            ui.label(
                egui::RichText::new("Servizi, conferme e integrità del sistema")
                    .small()
                    .weak(),
            );
        });
        ui.add_space(8.0);
        let audit_color = match audit_status {
            "Verificato" => POSITIVE,
            "Da controllare" => NEGATIVE,
            "Parziale" => CAUTION,
            _ => ACCENT,
        };
        let metrics = [
            ("CORE", core_status.as_str(), version.as_str(), core_color),
            (
                "INTELLIGENZA ARTIFICIALE",
                ai_status.as_str(),
                "Stato del servizio",
                ai_color,
            ),
            (
                "CONFERME",
                confirmations.as_str(),
                "Operazioni in attesa",
                if confirmations == "0" {
                    POSITIVE
                } else if confirmations == "—" {
                    ACCENT
                } else {
                    CAUTION
                },
            ),
            (
                "NOTIFICHE",
                unread.as_str(),
                "Da leggere",
                if unread == "0" {
                    POSITIVE
                } else if unread == "—" {
                    ACCENT
                } else {
                    CAUTION
                },
            ),
            ("AUTOMAZIONI", automations.as_str(), "Attive", ACCENT),
            (
                "INTEGRITÀ AUDIT",
                audit_status,
                "Registro locale",
                audit_color,
            ),
        ];
        let metrics_width = ui.available_width();
        let columns = if metrics_width >= 900.0 {
            3
        } else if metrics_width >= 560.0 {
            2
        } else {
            1
        };
        let gap = 12.0;
        let card_width = (metrics_width - gap * (columns - 1) as f32) / columns as f32;
        for row in metrics.chunks(columns) {
            ui.horizontal(|ui| {
                for (label, value, detail, color) in row {
                    egui::Frame::new()
                        .fill(RAISED)
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(
                                color.r(),
                                color.g(),
                                color.b(),
                                92,
                            ),
                        ))
                        .corner_radius(14)
                        .inner_margin(13)
                        .show(ui, |ui| {
                            ui.set_width((card_width - 28.0).max(80.0));
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("●").color(*color).small());
                                ui.label(egui::RichText::new(*label).small().strong().weak());
                            });
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(*value)
                                    .size(23.0)
                                    .strong()
                                    .color(*color),
                            );
                            ui.small(egui::RichText::new(*detail).weak());
                        });
                }
            });
            ui.add_space(gap);
        }

        self.show_dashboard_backends(ui, ctx);

        if ui.available_width() >= 820.0 {
            ui.columns(2, |columns| {
                self.show_dashboard_tasks(&mut columns[0], ctx);
                self.show_dashboard_notifications(&mut columns[1], ctx);
            });
        } else {
            self.show_dashboard_tasks(ui, ctx);
            ui.add_space(12.0);
            self.show_dashboard_notifications(ui, ctx);
        }
    }

    fn show_dashboard_backends(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(29, 49, 65)))
            .corner_radius(16)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Core collegati");
                    ui.label(
                        egui::RichText::new("Stato e riepiloghi in sola lettura")
                            .small()
                            .weak(),
                    );
                    if ui
                        .add_enabled(!self.loading, egui::Button::new("Aggiorna"))
                        .clicked()
                    {
                        self.data = None;
                        self.refresh(ctx);
                    }
                });
                if let Some(error) = &self.backend_config_error {
                    ui.colored_label(NEGATIVE, error);
                }
                let summaries = self
                    .data
                    .as_ref()
                    .and_then(|data| data["connected_backends"].as_array())
                    .cloned()
                    .unwrap_or_default();
                for summary in summaries {
                    let name = summary["name"].as_str().unwrap_or("Core");
                    let origin = summary["origin"].as_str().unwrap_or("");
                    let online = summary["online"] == true;
                    let removable = self.backends.iter().any(|backend| backend.origin == origin);
                    let ai_status = if !online {
                        "non verificabile"
                    } else if summary["ai_online"] == true {
                        "operativa"
                    } else {
                        "non disponibile"
                    };
                    let recent_tasks = if online {
                        summary["recent_tasks"]
                            .as_array()
                            .map(|tasks| tasks.len().to_string())
                            .unwrap_or_else(|| "—".into())
                    } else {
                        "—".into()
                    };
                    let unread = if online {
                        summary["notifications_unread"]
                            .as_u64()
                            .map(|count| count.to_string())
                            .unwrap_or_else(|| "—".into())
                    } else {
                        "—".into()
                    };
                    egui::Frame::new()
                        .fill(RAISED)
                        .corner_radius(12)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(egui::RichText::new("●").color(if online {
                                    POSITIVE
                                } else {
                                    NEGATIVE
                                }));
                                ui.strong(name);
                                ui.label(if online { "Online" } else { "Offline" });
                                if removable
                                    && ui
                                        .add_enabled(!self.loading, egui::Button::new("Dimentica"))
                                        .clicked()
                                {
                                    self.pending_backend_removal = Some(origin.to_owned());
                                }
                            });
                            ui.small(
                                egui::RichText::new(format!(
                                    "AI {} · {} attività recenti · {} notifiche non lette",
                                    ai_status, recent_tasks, unread,
                                ))
                                .weak(),
                            );
                            if let Some(error) = summary["error"].as_str() {
                                ui.colored_label(CAUTION, error);
                            }
                        });
                }
                egui::CollapsingHeader::new("Associa un altro Core")
                    .id_salt("add-core-backend")
                    .show(ui, |ui| {
                        ui.label(
                            "Usa l’origine HTTPS e il codice monouso generato in Dispositivi.",
                        );
                        ui.add_sized(
                            [ui.available_width(), 38.0],
                            egui::TextEdit::singleline(&mut self.backend_url)
                                .hint_text("https://jarvis.example.com"),
                        );
                        ui.add_sized(
                            [ui.available_width(), 38.0],
                            egui::TextEdit::singleline(&mut self.backend_pair_code)
                                .password(true)
                                .hint_text("Codice monouso"),
                        );
                        if let Some(error) = &self.backend_pair_error {
                            ui.colored_label(NEGATIVE, error);
                        }
                        if ui
                            .add_enabled(
                                !self.backend_pairing
                                    && !self.loading
                                    && self.backend_config_error.is_none(),
                                egui::Button::new(if self.backend_pairing {
                                    "Associazione in corso…"
                                } else {
                                    "Associa Core"
                                }),
                            )
                            .clicked()
                        {
                            self.add_core_backend(ctx);
                        }
                    });
            });

        if let Some(origin) = self.pending_backend_removal.clone() {
            egui::Window::new("Dimenticare questo Core?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!("Rimuovere le credenziali salvate per {origin}?"));
                    ui.small(
                        "L'autorizzazione sul Core remoto resta attiva: revocala in Dispositivi.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Annulla").clicked() {
                            self.pending_backend_removal = None;
                        }
                        if ui
                            .add_enabled(!self.loading, egui::Button::new("Dimentica"))
                            .clicked()
                        {
                            self.forget_core_backend(&origin, ctx);
                        }
                    });
                });
        }
    }

    fn show_dashboard_tasks(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Frame::new()
            .fill(SURFACE)
            .corner_radius(16)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.heading("Attività recenti");
                    if ui.button("Mostra tutte").clicked() {
                        self.current = "tasks".into();
                        self.data = None;
                        self.refresh(ctx);
                    }
                });
                let tasks = self
                    .data
                    .as_ref()
                    .and_then(|data| data["recent_tasks"].as_array())
                    .cloned()
                    .unwrap_or_default();
                if tasks.is_empty() {
                    ui.label(if self.loading {
                        "Caricamento delle attività…"
                    } else {
                        "Nessuna attività recente."
                    });
                }
                for task in tasks {
                    let objective = task["objective"]
                        .as_str()
                        .unwrap_or("Attività senza titolo")
                        .to_owned();
                    let state = task["state"].as_str().unwrap_or_default();
                    let (state_label, state_color) = dashboard_task_state(state);
                    let created_at = task["created_at"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(16)
                        .collect::<String>();
                    let backend = task["backend"].as_str().unwrap_or("Core");
                    egui::Frame::new()
                        .fill(RAISED)
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(
                                state_color.r(),
                                state_color.g(),
                                state_color.b(),
                                72,
                            ),
                        ))
                        .corner_radius(12)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                egui::Frame::new()
                                    .fill(egui::Color32::from_rgba_unmultiplied(
                                        state_color.r(),
                                        state_color.g(),
                                        state_color.b(),
                                        24,
                                    ))
                                    .corner_radius(egui::CornerRadius::same(8))
                                    .inner_margin(egui::Margin::symmetric(8, 4))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(state_label)
                                                .small()
                                                .strong()
                                                .color(state_color),
                                        );
                                    });
                                ui.label(egui::RichText::new(backend).small().weak());
                                if !created_at.is_empty() {
                                    ui.label(egui::RichText::new(&created_at).small().weak());
                                }
                            });
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(&objective).strong());
                            ui.add_space(6.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("Prepara ripresa in chat")
                                            .small()
                                            .strong()
                                            .color(ACCENT),
                                    )
                                    .fill(SELECTED),
                                )
                                .clicked()
                            {
                                self.chat_input = format!("Riprendi l'attività: {objective}");
                                self.current = "chat".into();
                            }
                        });
                    ui.add_space(8.0);
                }
            });
    }

    fn show_dashboard_notifications(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(29, 49, 65)))
            .corner_radius(16)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Notifiche");
                    if self.loading {
                        ui.add(egui::Spinner::new());
                    }
                    let has_unread = self
                        .notifications
                        .iter()
                        .any(|item| item["read_at"].is_null());
                    if ui
                        .add_enabled(has_unread, egui::Button::new("Segna tutte come lette"))
                        .clicked()
                    {
                        self.act(ctx, "/api/notifications/read-all".into(), None);
                    }
                });
                ui.small(
                    "Elenco del Core attivo; il conteggio in alto somma i Core raggiungibili.",
                );
                ui.add_space(4.0);
                self.show_notifications(ui);
            });
    }

    fn dashboard_intro(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        core_status: &str,
        ai_status: &str,
        core_color: egui::Color32,
        ai_color: egui::Color32,
    ) {
        ui.label(
            egui::RichText::new("JARVIS  /  CENTRO DI CONTROLLO")
                .small()
                .strong()
                .color(ACCENT),
        );
        ui.add_space(10.0);
        ui.heading(egui::RichText::new("JARVIS, in un colpo d'occhio.").size(29.0));
        ui.label(
            egui::RichText::new(
                "Un quadro chiaro dei tuoi Core, delle attività e delle notifiche.",
            )
            .size(15.0)
            .color(egui::Color32::from_rgb(180, 203, 222)),
        );
        ui.add_space(14.0);
        ui.horizontal_wrapped(|ui| {
            for (label, value, color) in [
                ("CORE", core_status, core_color),
                ("AI", ai_status, ai_color),
            ] {
                egui::Frame::new()
                    .fill(RAISED)
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 100),
                    ))
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("●").color(color).small());
                            ui.label(
                                egui::RichText::new(format!("{label}  {value}"))
                                    .small()
                                    .strong(),
                            );
                        });
                    });
            }
        });
        ui.add_space(8.0);
        ui.checkbox(&mut self.reduce_motion, "Riduci animazioni");
        if self
            .data
            .as_ref()
            .is_some_and(|data| data["first_run"] == true)
            && ui.button("Completa la configurazione iniziale").clicked()
        {
            self.current = "system".into();
            self.data = None;
            self.refresh(ctx);
        }
    }

    fn dashboard_actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(
            egui::RichText::new("AZIONI RAPIDE")
                .small()
                .strong()
                .color(ACCENT),
        );
        ui.add_space(6.0);

        let stacked = ui.available_width() < 230.0;
        if stacked {
            if ui
                .add_sized(
                    [ui.available_width(), 40.0],
                    egui::Button::new(egui::RichText::new("Nuova chat").strong().color(ACCENT))
                        .fill(SELECTED),
                )
                .clicked()
            {
                self.current = "chat".into();
            }
            if ui
                .add_sized([ui.available_width(), 38.0], egui::Button::new("Attività"))
                .clicked()
            {
                self.current = "tasks".into();
                self.data = None;
                self.refresh(ctx);
            }
            if ui
                .add_sized([ui.available_width(), 38.0], egui::Button::new("Sistema"))
                .clicked()
            {
                self.current = "system".into();
                self.data = None;
                self.refresh(ctx);
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Nuova chat").strong().color(ACCENT))
                            .fill(SELECTED)
                            .min_size(egui::vec2(132.0, 40.0)),
                    )
                    .clicked()
                {
                    self.current = "chat".into();
                }
                if ui
                    .add(egui::Button::new("Attività").min_size(egui::vec2(112.0, 38.0)))
                    .clicked()
                {
                    self.current = "tasks".into();
                    self.data = None;
                    self.refresh(ctx);
                }
                if ui
                    .add(egui::Button::new("Sistema").min_size(egui::vec2(112.0, 38.0)))
                    .clicked()
                {
                    self.current = "system".into();
                    self.data = None;
                    self.refresh(ctx);
                }
            });
        }
    }

    fn show_memory(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut search_clicked = false;
        let mut show_all_clicked = false;
        ui.horizontal_wrapped(|ui| {
            ui.heading("Memoria");
            ui.add(
                egui::TextEdit::singleline(&mut self.memory_search)
                    .desired_width(240.0)
                    .interactive(!self.loading)
                    .hint_text("Cerca nei ricordi"),
            );
            if ui
                .add_enabled(!self.loading, egui::Button::new("Cerca"))
                .clicked()
            {
                search_clicked = true;
            }
            if ui
                .add_enabled(
                    !self.loading && !self.memory_search.is_empty(),
                    egui::Button::new("Mostra tutti"),
                )
                .clicked()
            {
                show_all_clicked = true;
            }
            if self.loading {
                ui.add(egui::Spinner::new());
            }
        });
        if show_all_clicked {
            self.memory_search.clear();
        }
        if search_clicked || show_all_clicked {
            self.data = None;
            self.refresh(ctx);
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
            if items.is_empty() {
                ui.label(if self.memory_search.trim().is_empty() {
                    "Nessun ricordo salvato."
                } else {
                    "Nessun ricordo corrisponde alla ricerca."
                });
            }
            for item in items {
                self.show_item(ui, ctx, &item);
            }
        } else if !self.loading {
            ui.label("I ricordi restano sul Core; la ricerca usa gli indici locali.");
        }
    }

    fn show_setup(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Configurazione di JARVIS");
        ui.label("La chat è disponibile subito. Configura la voce locale quando ti serve.");
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        let Some(data) = self.data.clone() else {
            if self.loading {
                ui.spinner();
            }
            if ui.button("Rileggi configurazione").clicked() {
                self.refresh(ctx);
            }
            return;
        };
        let running = data["running"] == true;
        if let Some(saved_steps) = data["saved"]["steps"].as_array() {
            if saved_steps.iter().all(|step| step["status"] == "done") {
                ui.label("Configurazione voce completata");
            } else {
                ui.label("Configurazione voce parziale. Puoi riprovare.");
            }
        }
        ui.add_enabled_ui(!running && !self.loading, |ui| {
            for (field, label, choice) in [
                ("stt_models", "Riconoscimento vocale", &mut self.setup_stt),
                ("tts_voices", "Voce", &mut self.setup_tts),
            ] {
                ui.small(label);
                egui::ComboBox::from_id_salt(field)
                    .selected_text(choice.as_str())
                    .show_ui(ui, |ui| {
                        if let Some(options) = data["choices"][field].as_array() {
                            for option in options.iter().filter_map(Value::as_str) {
                                ui.selectable_value(choice, option.into(), option);
                            }
                        }
                    });
            }
            if let (Some(packages), Some(stt), Some(tts)) = (
                data["sizes_mb"]["voice_packages"].as_u64(),
                data["sizes_mb"]["stt"][&self.setup_stt].as_u64(),
                data["sizes_mb"]["tts"].as_u64(),
            ) {
                ui.label(format!("Download stimato: {} MB", packages + stt + tts));
            }
            if ui.button("Configura voce locale").clicked() {
                self.act(
                    ctx,
                    "/api/setup/start".into(),
                    Some(json!({
                        "stt_model": self.setup_stt, "tts_voice": self.setup_tts
                    })),
                );
            }
        });
        if running {
            ui.spinner();
            if !self.loading && self.last_load.elapsed() >= Duration::from_secs(1) {
                self.refresh(ctx);
            }
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        if let Some(steps) = data["steps"]
            .as_array()
            .filter(|steps| !steps.is_empty())
            .or_else(|| data["saved"]["steps"].as_array())
        {
            for step in steps {
                ui.group(|ui| {
                    ui.strong(
                        step["label"]
                            .as_str()
                            .or_else(|| step["id"].as_str())
                            .unwrap_or("Configurazione"),
                    );
                    ui.label(match step["status"].as_str() {
                        Some("done") => "Completato",
                        Some("running") => "In corso",
                        Some("error" | "failed") => "Non riuscito",
                        Some("skipped") => "Saltato",
                        _ => "In attesa",
                    });
                    if let Some(progress) = step["progress"].as_f64() {
                        ui.add(egui::ProgressBar::new((progress / 100.0) as f32).show_percentage());
                    }
                    if let Some(detail) =
                        step["detail"].as_str().filter(|detail| !detail.is_empty())
                    {
                        ui.small(detail);
                    }
                });
            }
        }
        ui.separator();
        if ui.button("Configura provider AI").clicked() {
            self.current = "models".into();
            self.data = None;
            self.refresh(ctx);
        }
        if ui.button("Crea backup cifrato").clicked() {
            self.act(ctx, "/api/backup".into(), Some(json!({})));
        }
    }

    fn show_connectors(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Connettori");
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        if let Some(notice) = &self.connector_oauth_notice {
            ui.colored_label(POSITIVE, notice);
        }
        if ui
            .add_enabled(!self.loading, egui::Button::new("Aggiorna"))
            .clicked()
        {
            self.refresh(ctx);
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        let installed = data["items"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        if let Some(catalog) = data["catalog"].as_array() {
            egui::ComboBox::from_id_salt("connector-catalog")
                .selected_text(
                    catalog
                        .iter()
                        .find(|entry| entry["id"] == self.connector_template)
                        .and_then(|entry| entry["name"].as_str())
                        .unwrap_or("Scegli dal catalogo"),
                )
                .show_ui(ui, |ui| {
                    for entry in catalog {
                        if let (Some(id), Some(name)) =
                            (entry["id"].as_str(), entry["name"].as_str())
                        {
                            if ui
                                .selectable_value(&mut self.connector_template, id.into(), name)
                                .clicked()
                            {
                                self.connector_client_id.clear();
                                self.connector_credential.clear();
                                self.connector_oauth_notice = None;
                            }
                        }
                    }
                });
            if let Some(entry) = catalog
                .iter()
                .find(|entry| entry["id"] == self.connector_template)
            {
                ui.label(entry["description"].as_str().unwrap_or_default());
                if let Some(instructions) = entry["setup"].as_str() {
                    ui.small(instructions);
                }
                if !installed
                    .iter()
                    .any(|item| item["id"] == self.connector_template)
                {
                    let needs_id = entry["needs_client_id"] == true;
                    if needs_id {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.connector_client_id)
                                .hint_text("ID client OAuth"),
                        );
                    }
                    if ui
                        .add_enabled(
                            !self.loading
                                && (!needs_id || !self.connector_client_id.trim().is_empty()),
                            egui::Button::new("Installa"),
                        )
                        .clicked()
                    {
                        self.act(
                            ctx,
                            format!("/api/connectors/templates/{}", self.connector_template),
                            Some(json!({"client_id": self.connector_client_id.trim()})),
                        );
                    }
                }
            }
        }
        for item in installed {
            ui.group(|ui| {
                let id = item["id"].as_str().unwrap_or_default();
                ui.strong(item["name"].as_str().unwrap_or(id));
                ui.small(if item["authorized"] == true {
                    "Autorizzato"
                } else {
                    "Da autorizzare"
                });
                if item["auth"] == "none" || id.is_empty() {
                    return;
                }
                if self.connector_template != id {
                    if ui.button("Configura accesso").clicked() {
                        self.connector_template = id.into();
                        self.connector_credential.clear();
                        self.connector_oauth_notice = None;
                    }
                    return;
                }
                let oauth = item["auth"] == "oauth2";
                ui.add(
                    egui::TextEdit::singleline(&mut self.connector_credential)
                        .password(true)
                        .hint_text(if oauth {
                            "Client secret OAuth, se richiesto"
                        } else {
                            "Credenziale del servizio"
                        }),
                );
                if ui
                    .add_enabled(
                        !self.loading && (oauth || !self.connector_credential.trim().is_empty()),
                        egui::Button::new("Autorizza"),
                    )
                    .clicked()
                {
                    self.error = None;
                    self.connector_oauth_notice = None;
                    if oauth {
                        if let Some(client) = self.client.clone() {
                            let credential = std::mem::take(&mut self.connector_credential);
                            let tx = self.tx.clone();
                            let path = format!("/api/connectors/{id}/oauth/start");
                            self.loading = true;
                            std::thread::spawn(move || {
                                let result =
                                    client.post(&path, Some(&json!({"client_secret": credential})));
                                let _ = tx.send(Event::ConnectorOAuth(result));
                            });
                        } else {
                            self.error = Some("Core non connesso.".into());
                        }
                    } else {
                        let credential = std::mem::take(&mut self.connector_credential);
                        self.act(
                            ctx,
                            format!("/api/connectors/{id}/authorize"),
                            Some(json!({"credential": credential})),
                        );
                    }
                }
                if item["authorized"] == true
                    && ui
                        .add_enabled(!self.loading, egui::Button::new("Revoca accesso"))
                        .clicked()
                {
                    self.act(ctx, format!("/api/connectors/{id}/revoke"), Some(json!({})));
                }
            });
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
        let pairing_urls: Vec<String> = self
            .data
            .as_ref()
            .and_then(|data| data["pairing_urls"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        self.pairing_url = available_pairing_url(&self.pairing_url, &pairing_urls);
        let mut pairing_url = self.pairing_url.clone();
        let invite = self.device_invite.clone();
        ui.group(|ui| {
            ui.heading("Associa un dispositivo");
            ui.label("1. Apri JARVIS sul dispositivo da associare.");
            ui.label("2. Inserisci l’indirizzo HTTPS del Core e il codice monouso.");
            if pairing_urls.len() > 1 {
                egui::ComboBox::from_id_salt("pairing-host")
                    .selected_text(pairing_url.as_str())
                    .show_ui(ui, |ui| {
                        for url in &pairing_urls {
                            ui.selectable_value(&mut pairing_url, url.clone(), url.as_str());
                        }
                    });
            }
            if !pairing_url.is_empty() {
                ui.label(format!("Indirizzo del Core: {pairing_url}"));
            } else {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 197, 108),
                    "Configura prima un host HTTPS autorizzato nel Core per associare dispositivi remoti.",
                );
            }
            ui.add_enabled(
                !self.invite_creating && invite.is_none(),
                egui::Checkbox::new(
                    &mut self.invite_confirm,
                    "Consenti le conferme (oltre alla chat)",
                ),
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
            if let Some(invite) = invite {
                let allows_confirm = invite["scopes"]
                    .as_array()
                    .is_some_and(|scopes| {
                        scopes
                            .iter()
                            .any(|scope| scope.as_str() == Some("confirm"))
                    });
                ui.label(format!(
                    "Codice: {}",
                    invite["code"].as_str().unwrap_or("—")
                ));
                if let Some(code) = invite["code"].as_str() {
                    if ui.button("Copia codice").clicked() {
                        ctx.copy_text(code.to_owned());
                    }
                    if !pairing_url.is_empty()
                        && ui.button("Copia istruzioni di associazione").clicked()
                    {
                        ctx.copy_text(format!(
                            "Indirizzo HTTPS del Core: {pairing_url}\nCodice monouso: {code}\nPermessi: {}",
                            if allows_confirm {
                                "chat e conferme"
                            } else {
                                "solo chat"
                            }
                        ));
                    }
                }
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
        self.pairing_url = pairing_url;
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
                        ui.add_space((ui.available_height() - 390.0).max(8.0) * 0.25);
                        ui.add(egui::Image::new(egui::load::SizedTexture::new(
                            self.startup_icon.id(),
                            egui::vec2(176.0, 176.0),
                        )));
                        ui.add_space(12.0);
                        ui.heading(egui::RichText::new("JARVIS").size(30.0).color(ACCENT));
                        ui.add_space(8.0);
                        if let Some(error) = self.error.clone() {
                            ui.label(egui::RichText::new("Avvio non riuscito").strong());
                            egui::Frame::new()
                                .fill(SURFACE)
                                .corner_radius(10)
                                .inner_margin(10)
                                .show(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .id_salt("startup-error")
                                        .max_height(84.0)
                                        .show(ui, |ui| ui.label(error));
                                });
                            if ui.button("Riprova").clicked() {
                                self.retry_connection(&ctx);
                            }
                        } else {
                            ui.add(egui::Spinner::new().size(22.0));
                            ui.label(if self.remote_url.is_empty() {
                                "Avvio del Core locale…"
                            } else {
                                "Connessione al Core remoto…"
                            });
                            ui.small("La finestra resta disponibile durante la connessione.");
                        }
                    });
                });
            }
            ctx.request_repaint_after(Duration::from_millis(250));
            return;
        }

        let compact_header = ctx.viewport_rect().width() < 720.0;
        egui::Panel::top("topbar").exact_size(62.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.heading(
                    egui::RichText::new("JARVIS")
                        .color(ACCENT)
                        .strong()
                        .size(20.0),
                );
                if !compact_header || ctx.viewport_rect().width() >= 460.0 {
                    ui.separator();
                    ui.label(egui::RichText::new(title(&self.current)).strong());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let unread = self
                        .notifications
                        .iter()
                        .filter(|notification| notification["read_at"].is_null())
                        .count();
                    let notification_label = if compact_header {
                        format!("Notifiche {unread}")
                    } else {
                        format!("Notifiche   {unread}")
                    };
                    if ui
                        .add(
                            egui::Button::new(notification_label).fill(RAISED).stroke(
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(44, 76, 96)),
                            ),
                        )
                        .clicked()
                    {
                        self.current = "dashboard".into();
                        self.data = None;
                        self.refresh(&ctx);
                    }
                    ui.add_space(8.0);
                    let core_label = if self.remote {
                        "Core remoto"
                    } else {
                        "Core locale"
                    };
                    if compact_header {
                        ui.label(
                            egui::RichText::new("●")
                                .color(if self.remote { ACCENT } else { POSITIVE })
                                .strong(),
                        )
                        .on_hover_text(core_label);
                    } else {
                        egui::Frame::new()
                            .fill(if self.remote {
                                RAISED
                            } else {
                                egui::Color32::from_rgb(15, 48, 44)
                            })
                            .stroke(egui::Stroke::new(
                                1.0,
                                if self.remote {
                                    egui::Color32::from_rgb(44, 76, 96)
                                } else {
                                    egui::Color32::from_rgb(40, 100, 79)
                                },
                            ))
                            .corner_radius(egui::CornerRadius::same(16))
                            .inner_margin(egui::Margin::symmetric(10, 5))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("●").color(if self.remote {
                                        ACCENT
                                    } else {
                                        POSITIVE
                                    }));
                                    ui.label(
                                        egui::RichText::new(if self.remote {
                                            "CORE REMOTO"
                                        } else {
                                            "CORE LOCALE"
                                        })
                                        .small()
                                        .strong(),
                                    );
                                });
                            });
                    }
                });
            });
        });

        let compact = ctx.viewport_rect().width() < 720.0;
        if compact {
            egui::Panel::top("compact-navigation")
                .exact_size(52.0)
                .show(ui, |ui| {
                    egui::ScrollArea::horizontal()
                        .id_salt("compact-navigation-list")
                        .show(ui, |ui| ui.horizontal(|ui| self.navigation(ui, true, &ctx)));
                });
        } else {
            egui::Panel::left("navigation")
                .resizable(false)
                .exact_size(190.0)
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
                                .show(ui, |ui| self.navigation(ui, false, &ctx));
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
        }
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

        if self.current != "chat"
            && !self.backend_pairing
            && self.last_load.elapsed() > Duration::from_secs(15)
        {
            self.refresh(&ctx);
        }
        if self.core.as_mut().is_some_and(|core| !core.is_alive()) {
            self.error = Some("Il Core si è arrestato. Riavvia JARVIS per riconnetterti.".into());
        }
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

pub(crate) fn application_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../icons/icon.png"))
        .expect("L'icona PNG incorporata di JARVIS deve essere valida.")
}

fn normalize_core_origin(value: &str) -> Result<String, String> {
    let mut url = url::Url::parse(value.trim()).map_err(|_| "Indirizzo del Core non valido")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err("Inserisci l’origine del Core, senza percorso né credenziali".into());
    }
    url.set_path("/");
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn backend_display_name(origin: &str) -> String {
    url::Url::parse(origin)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "Core associato".into())
}

fn load_core_backends(path: &Path) -> Result<Vec<CoreBackend>, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Configurazione Core non leggibile: {error}")),
    };
    let config: Value = serde_json::from_str(&content)
        .map_err(|error| format!("Configurazione Core non valida: {error}"))?;
    if config["version"] != 1 {
        return Err("Versione della configurazione Core non supportata.".into());
    }
    let entries = config["backends"]
        .as_array()
        .ok_or("Elenco dei Core associati non valido")?;
    let mut backends = Vec::with_capacity(entries.len());
    for entry in entries {
        let origin = entry["origin"]
            .as_str()
            .ok_or("Origine del Core associato mancante")?;
        let origin = normalize_core_origin(origin)?;
        if !is_https_url(&origin) {
            return Err("Un Core associato deve usare HTTPS.".into());
        }
        let token = entry["token"]
            .as_str()
            .filter(|token| !token.trim().is_empty())
            .ok_or("Credenziale del Core associato mancante")?
            .to_owned();
        ApiClient::device(&origin, token.clone())?;
        if backends
            .iter()
            .any(|backend: &CoreBackend| backend.origin == origin)
        {
            return Err("La configurazione contiene Core duplicati.".into());
        }
        backends.push(CoreBackend { origin, token });
    }
    Ok(backends)
}

fn save_core_backends(path: &Path, backends: &[CoreBackend]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("Cartella della configurazione Core non valida")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let content = serde_json::to_vec(&json!({
        "version": 1,
        "backends": backends.iter().map(|backend| json!({
            "origin": backend.origin,
            "token": backend.token,
        })).collect::<Vec<_>>(),
    }))
    .map_err(|error| error.to_string())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        if !path.exists() {
            options.mode(0o600);
        } else {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    file.write_all(&content)
        .map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn load_dashboard(
    client: &ApiClient,
    primary_name: &str,
    primary_origin: &str,
    backends: &[CoreBackend],
) -> Result<Value, String> {
    let mut data = client.get("/api/health")?;
    if let Ok(setup) = client.get("/api/setup") {
        data["first_run"] = setup["first_run"].clone();
    }
    let primary_tasks = client.get("/api/tasks?limit=4")?;
    let primary_tasks = primary_tasks["items"]
        .as_array()
        .cloned()
        .ok_or("Risposta attività del Core principale non valida")?;
    let mut summaries = vec![dashboard_backend_summary(
        primary_name,
        primary_origin,
        &data,
        primary_tasks,
        None,
    )];

    let peer_summaries = std::thread::scope(|scope| {
        let workers: Vec<_> = backends
            .iter()
            .map(|backend| scope.spawn(move || fetch_backend_summary(backend)))
            .collect();
        workers
            .into_iter()
            .enumerate()
            .map(|(index, worker)| {
                worker.join().unwrap_or_else(|_| {
                    unavailable_backend_summary(
                        &backend_display_name(&backends[index].origin),
                        &backends[index].origin,
                        "Aggiornamento del Core non riuscito.",
                    )
                })
            })
            .collect::<Vec<_>>()
    });
    summaries.extend(peer_summaries);
    merge_dashboard_summaries(&mut data, summaries);
    Ok(data)
}

fn fetch_backend_summary(backend: &CoreBackend) -> Value {
    let name = backend_display_name(&backend.origin);
    let client = match ApiClient::device_with_timeout(
        &backend.origin,
        backend.token.clone(),
        Duration::from_secs(4),
    ) {
        Ok(client) => client,
        Err(_) => {
            return unavailable_backend_summary(
                &name,
                &backend.origin,
                "Connessione al Core non riuscita.",
            );
        }
    };
    let health = match client.get("/api/health") {
        Ok(health) => health,
        Err(_) => {
            return unavailable_backend_summary(
                &name,
                &backend.origin,
                "Core non raggiungibile o non autorizzato.",
            );
        }
    };
    let (tasks, error) = match client.get("/api/tasks?limit=4") {
        Ok(response) => match response["items"].as_array() {
            Some(items) => (items.clone(), None),
            None => (Vec::new(), Some("Riepilogo attività non disponibile.")),
        },
        Err(_) => (Vec::new(), Some("Riepilogo attività non disponibile.")),
    };
    dashboard_backend_summary(&name, &backend.origin, &health, tasks, error)
}

fn unavailable_backend_summary(name: &str, origin: &str, error: &str) -> Value {
    json!({
        "name": name,
        "origin": origin,
        "online": false,
        "ai_online": false,
        "pending_confirmations": null,
        "notifications_unread": null,
        "automations": null,
        "audit_valid": null,
        "recent_tasks": null,
        "error": error,
    })
}

fn dashboard_backend_summary(
    name: &str,
    origin: &str,
    health: &Value,
    tasks: Vec<Value>,
    error: Option<&str>,
) -> Value {
    let online = health["core"]["status"] == "up";
    let tasks: Vec<_> = tasks
        .into_iter()
        .map(|task| {
            json!({
                "objective": task["objective"],
                "state": task["state"],
                "created_at": task["created_at"],
                "backend": name,
            })
        })
        .collect();
    let recent_tasks = if online && error.is_none() {
        Value::Array(tasks)
    } else {
        Value::Null
    };
    json!({
        "name": name,
        "origin": origin,
        "online": online,
        "ai_online": online && health["ai"]["status"] == "up",
        "pending_confirmations": health["pending_confirmations"].as_u64().filter(|_| online),
        "notifications_unread": health["notifications_unread"].as_u64().filter(|_| online),
        "automations": health["automations"].as_u64().filter(|_| online),
        "audit_valid": health["audit"]["chain_valid"].as_bool(),
        "recent_tasks": recent_tasks,
        "error": error,
    })
}

fn merge_dashboard_summaries(data: &mut Value, summaries: Vec<Value>) {
    let total = summaries.len() as u64;
    let online = summaries
        .iter()
        .filter(|summary| summary["online"] == true)
        .count() as u64;
    let ai_online = summaries
        .iter()
        .filter(|summary| summary["ai_online"] == true)
        .count() as u64;
    let sum = |key: &str| {
        summaries
            .iter()
            .filter(|summary| summary["online"] == true)
            .map(|summary| summary[key].as_u64().unwrap_or(0))
            .sum::<u64>()
    };
    let pending_confirmations = sum("pending_confirmations");
    let notifications_unread = sum("notifications_unread");
    let automations = sum("automations");
    let all_audits_valid = online == total
        && summaries
            .iter()
            .all(|summary| summary["audit_valid"] == true);
    let mut tasks: Vec<Value> = summaries
        .iter()
        .flat_map(|summary| {
            summary["recent_tasks"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned()
        })
        .collect();
    tasks.sort_by(|left, right| {
        right["created_at"]
            .as_str()
            .cmp(&left["created_at"].as_str())
    });
    tasks.truncate(4);
    let core_label = if total > 1 {
        format!("{online}/{total} online")
    } else {
        data["core"]["status"].as_str().unwrap_or("—").to_owned()
    };
    let ai_label = if total > 1 {
        format!("{ai_online}/{online} operativi")
    } else {
        data["ai"]["status"].as_str().unwrap_or("—").to_owned()
    };
    data["pending_confirmations"] = pending_confirmations.into();
    data["notifications_unread"] = notifications_unread.into();
    data["automations"] = automations.into();
    data["audit"]["chain_valid"] = if online == total {
        Value::Bool(all_audits_valid)
    } else {
        Value::Null
    };
    data["recent_tasks"] = tasks.into();
    data["connected_backends"] = summaries.into();
    data["aggregate"] = json!({
        "total": total,
        "online": online,
        "ai_online": ai_online,
        "core_label": core_label,
        "ai_label": ai_label,
        "all_online": online == total,
    });
}

fn spawn_connection(tx: Sender<Event>) {
    std::thread::spawn(move || {
        let result = connect_core();
        let _ = tx.send(Event::Connected(Box::new(result)));
    });
}

fn service_color(status: &str) -> egui::Color32 {
    match status.to_ascii_lowercase().as_str() {
        "up" | "active" | "ready" | "online" | "ok" | "healthy" | "configured" => POSITIVE,
        "starting" | "degraded" | "unknown" => CAUTION,
        "down" | "error" | "failed" | "offline" | "unavailable" | "stopped" => NEGATIVE,
        _ => ACCENT,
    }
}

fn availability_color(online: Option<u64>, total: Option<u64>) -> Option<egui::Color32> {
    match (online, total) {
        (Some(online), Some(total)) if total > 0 && online == total => Some(POSITIVE),
        (Some(0), Some(total)) if total > 0 => Some(NEGATIVE),
        (Some(_), Some(total)) if total > 0 => Some(CAUTION),
        _ => None,
    }
}

fn dashboard_core_color(data: &Value) -> egui::Color32 {
    availability_color(
        data["aggregate"]["online"].as_u64(),
        data["aggregate"]["total"].as_u64(),
    )
    .or_else(|| data["core"]["status"].as_str().map(service_color))
    .unwrap_or(ACCENT)
}

fn dashboard_ai_color(data: &Value, status: &str) -> egui::Color32 {
    availability_color(
        data["aggregate"]["ai_online"].as_u64(),
        data["aggregate"]["total"].as_u64(),
    )
    .unwrap_or_else(|| service_color(status))
}

fn dashboard_task_state(state: &str) -> (&'static str, egui::Color32) {
    match state {
        "pending" => ("In attesa", ACCENT),
        "planned" => ("Pianificata", ACCENT),
        "running" => ("In corso", ACCENT),
        "waiting_confirmation" => ("Richiede conferma", CAUTION),
        "blocked" => ("Bloccata", CAUTION),
        "succeeded" => ("Completata", POSITIVE),
        "failed" => ("Non riuscita", NEGATIVE),
        "cancelled" => ("Annullata", egui::Color32::from_rgb(157, 176, 191)),
        _ => ("Stato non riconosciuto", CAUTION),
    }
}

fn show_avatar(ui: &mut egui::Ui, ctx: &egui::Context, reduce_motion: bool) {
    let side = ui.available_width().clamp(132.0, 236.0);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let time = if reduce_motion {
        0.0
    } else {
        ctx.input(|input| input.time)
    };
    let pulse = if reduce_motion {
        0.0
    } else {
        ((time * 1.15).sin() * 0.5 + 0.5) as f32
    };
    let breath = pulse * 3.0;
    let accent = egui::Color32::from_rgb(35, 198, 244);

    painter.circle_filled(center, side * 0.47, egui::Color32::from_rgb(8, 24, 39));
    painter.circle_filled(
        center,
        side * 0.39,
        egui::Color32::from_rgba_unmultiplied(13, 89, 117, 18 + (pulse * 10.0) as u8),
    );
    painter.circle_stroke(
        center,
        side * 0.43 + breath,
        egui::Stroke::new(1.4, egui::Color32::from_rgba_unmultiplied(35, 198, 244, 75)),
    );
    painter.circle_stroke(
        center,
        side * 0.36 - breath * 0.35,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 83, 112)),
    );
    painter.circle_stroke(
        center,
        side * 0.31,
        egui::Stroke::new(0.7, egui::Color32::from_rgba_unmultiplied(51, 125, 151, 90)),
    );
    for index in 0..12 {
        let angle = std::f32::consts::TAU * index as f32 / 12.0;
        let direction = egui::Vec2::new(angle.cos(), angle.sin());
        let inner = if index % 3 == 0 { 0.395 } else { 0.407 };
        painter.line_segment(
            [
                center + direction * side * inner,
                center + direction * side * 0.435,
            ],
            egui::Stroke::new(
                if index % 3 == 0 { 1.3 } else { 0.8 },
                egui::Color32::from_rgba_unmultiplied(
                    61,
                    143,
                    171,
                    if index % 3 == 0 { 200 } else { 110 },
                ),
            ),
        );
    }

    let unit = side / 236.0;
    let ring_radius = side * 0.40;
    let sweep = std::f32::consts::PI * 1.45;
    let start = time as f32 * 0.28;
    let arc = (0..=48)
        .map(|index| {
            let angle = start + sweep * index as f32 / 48.0;
            center + egui::Vec2::new(angle.cos(), angle.sin()) * ring_radius
        })
        .collect();
    painter.add(egui::Shape::line(arc, egui::Stroke::new(2.0, accent)));
    for index in 0..3 {
        let angle = start + time as f32 * 0.7 + index as f32 * std::f32::consts::TAU / 3.0;
        let point = center + egui::Vec2::new(angle.cos(), angle.sin()) * ring_radius;
        painter.circle_filled(
            point,
            4.0 * unit,
            egui::Color32::from_rgba_unmultiplied(35, 198, 244, 30),
        );
        painter.circle_filled(point, 2.2 * unit, accent);
    }

    let face_center = center + egui::vec2(0.0, -side * 0.035);
    let shoulders = vec![
        center + egui::vec2(-side * 0.31, side * 0.31),
        center + egui::vec2(-side * 0.27, side * 0.22),
        center + egui::vec2(-side * 0.20, side * 0.17),
        center + egui::vec2(-side * 0.12, side * 0.14),
        center + egui::vec2(-side * 0.065, side * 0.18),
        center + egui::vec2(0.0, side * 0.205),
        center + egui::vec2(side * 0.065, side * 0.18),
        center + egui::vec2(side * 0.12, side * 0.14),
        center + egui::vec2(side * 0.20, side * 0.17),
        center + egui::vec2(side * 0.27, side * 0.22),
        center + egui::vec2(side * 0.31, side * 0.31),
    ];
    painter.add(egui::Shape::convex_polygon(
        shoulders,
        egui::Color32::from_rgb(13, 37, 54),
        egui::Stroke::new(1.7, egui::Color32::from_rgb(36, 149, 185)),
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + egui::vec2(-side * 0.19, side * 0.29),
            center + egui::vec2(-side * 0.15, side * 0.22),
            center + egui::vec2(-side * 0.075, side * 0.19),
            center + egui::vec2(-side * 0.12, side * 0.31),
        ],
        egui::Color32::from_rgb(20, 57, 75),
        egui::Stroke::new(1.0, egui::Color32::from_rgb(39, 133, 160)),
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + egui::vec2(side * 0.19, side * 0.29),
            center + egui::vec2(side * 0.15, side * 0.22),
            center + egui::vec2(side * 0.075, side * 0.19),
            center + egui::vec2(side * 0.12, side * 0.31),
        ],
        egui::Color32::from_rgb(20, 57, 75),
        egui::Stroke::new(1.0, egui::Color32::from_rgb(39, 133, 160)),
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + egui::vec2(-side * 0.055, side * 0.105),
            center + egui::vec2(side * 0.055, side * 0.105),
            center + egui::vec2(side * 0.065, side * 0.22),
            center + egui::vec2(0.0, side * 0.245),
            center + egui::vec2(-side * 0.065, side * 0.22),
        ],
        egui::Color32::from_rgb(17, 49, 66),
        egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 124, 151)),
    ));
    painter.line_segment(
        [
            center + egui::vec2(-side * 0.045, side * 0.205),
            center + egui::vec2(0.0, side * 0.225),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_rgb(71, 166, 188)),
    );
    painter.line_segment(
        [
            center + egui::vec2(side * 0.045, side * 0.205),
            center + egui::vec2(0.0, side * 0.225),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_rgb(71, 166, 188)),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            face_center + egui::vec2(-side * 0.075, side * 0.115),
            face_center + egui::vec2(-side * 0.12, side * 0.07),
            face_center + egui::vec2(-side * 0.105, side * 0.015),
            face_center + egui::vec2(-side * 0.075, side * 0.045),
        ],
        egui::Color32::from_rgb(20, 54, 72),
        egui::Stroke::new(0.8, egui::Color32::from_rgb(46, 131, 157)),
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            face_center + egui::vec2(side * 0.075, side * 0.115),
            face_center + egui::vec2(side * 0.12, side * 0.07),
            face_center + egui::vec2(side * 0.105, side * 0.015),
            face_center + egui::vec2(side * 0.075, side * 0.045),
        ],
        egui::Color32::from_rgb(20, 54, 72),
        egui::Stroke::new(0.8, egui::Color32::from_rgb(46, 131, 157)),
    ));

    let ear_y = face_center.y - side * 0.005;
    for direction in [-1.0, 1.0] {
        let ear = egui::pos2(face_center.x + direction * side * 0.15, ear_y);
        painter.circle_filled(ear, side * 0.035, egui::Color32::from_rgb(13, 39, 54));
        painter.circle_stroke(
            ear,
            side * 0.035,
            egui::Stroke::new(1.2 * unit, egui::Color32::from_rgb(47, 150, 178)),
        );
        painter.circle_filled(
            ear,
            side * 0.009,
            egui::Color32::from_rgba_unmultiplied(35, 198, 244, 170),
        );
    }
    let head = vec![
        face_center + egui::vec2(0.0, -side * 0.27),
        face_center + egui::vec2(side * 0.075, -side * 0.255),
        face_center + egui::vec2(side * 0.14, -side * 0.21),
        face_center + egui::vec2(side * 0.16, -side * 0.12),
        face_center + egui::vec2(side * 0.145, -side * 0.025),
        face_center + egui::vec2(side * 0.12, side * 0.075),
        face_center + egui::vec2(side * 0.075, side * 0.14),
        face_center + egui::vec2(0.0, side * 0.17),
        face_center + egui::vec2(-side * 0.075, side * 0.14),
        face_center + egui::vec2(-side * 0.12, side * 0.075),
        face_center + egui::vec2(-side * 0.145, -side * 0.025),
        face_center + egui::vec2(-side * 0.16, -side * 0.12),
        face_center + egui::vec2(-side * 0.14, -side * 0.21),
        face_center + egui::vec2(-side * 0.075, -side * 0.255),
    ];
    painter.add(egui::Shape::convex_polygon(
        head,
        egui::Color32::from_rgb(16, 40, 56),
        egui::Stroke::new(1.8, egui::Color32::from_rgb(71, 194, 224)),
    ));
    painter.add(egui::Shape::line(
        vec![
            face_center + egui::vec2(-side * 0.095, -side * 0.195),
            face_center + egui::vec2(-side * 0.045, -side * 0.22),
            face_center + egui::vec2(0.0, -side * 0.225),
            face_center + egui::vec2(side * 0.045, -side * 0.22),
            face_center + egui::vec2(side * 0.095, -side * 0.195),
        ],
        egui::Stroke::new(1.1, egui::Color32::from_rgb(61, 142, 165)),
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            face_center + egui::vec2(0.0, -side * 0.19),
            face_center + egui::vec2(side * 0.025, -side * 0.16),
            face_center + egui::vec2(0.0, -side * 0.125),
            face_center + egui::vec2(-side * 0.025, -side * 0.16),
        ],
        egui::Color32::from_rgb(24, 81, 102),
        egui::Stroke::new(0.8, egui::Color32::from_rgb(75, 195, 220)),
    ));
    let eye_y = face_center.y - side * 0.025;
    for direction in [-1.0, 1.0] {
        painter.line_segment(
            [
                face_center + egui::vec2(direction * side * 0.035, -side * 0.075),
                face_center + egui::vec2(direction * side * 0.09, -side * 0.068),
            ],
            egui::Stroke::new(1.2, egui::Color32::from_rgb(57, 134, 158)),
        );
        painter.line_segment(
            [
                face_center + egui::vec2(direction * side * 0.13, -side * 0.12),
                face_center + egui::vec2(direction * side * 0.115, side * 0.045),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(43, 106, 132)),
        );
    }

    for direction in [-1.0, 1.0] {
        let eye = egui::pos2(face_center.x + direction * side * 0.064, eye_y);
        painter.circle_filled(
            eye,
            side * 0.049,
            egui::Color32::from_rgba_unmultiplied(25, 167, 213, 24 + (pulse * 28.0) as u8),
        );
        painter.circle_stroke(
            eye,
            side * 0.035,
            egui::Stroke::new(0.8, egui::Color32::from_rgba_unmultiplied(35, 198, 244, 85)),
        );
        painter.line_segment(
            [
                egui::pos2(face_center.x + direction * side * 0.09, eye_y),
                egui::pos2(face_center.x + direction * side * 0.035, eye_y),
            ],
            egui::Stroke::new(2.8 * unit, accent),
        );
        painter.circle_filled(
            egui::pos2(face_center.x + direction * side * 0.064, eye_y),
            1.0 * unit + pulse * 0.5 * unit,
            egui::Color32::from_rgb(206, 250, 255),
        );
    }
    for direction in [-1.0, 1.0] {
        painter.line_segment(
            [
                egui::pos2(
                    face_center.x + direction * side * 0.118,
                    eye_y + side * 0.055,
                ),
                egui::pos2(
                    face_center.x + direction * side * 0.085,
                    eye_y + side * 0.09,
                ),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(59, 129, 154)),
        );
    }
    painter.add(egui::Shape::line(
        vec![
            egui::pos2(face_center.x, eye_y + side * 0.005),
            egui::pos2(face_center.x - side * 0.012, eye_y + side * 0.055),
            egui::pos2(face_center.x + side * 0.012, eye_y + side * 0.065),
        ],
        egui::Stroke::new(1.2 * unit, egui::Color32::from_rgb(105, 179, 197)),
    ));
    painter.add(egui::Shape::line(
        vec![
            egui::pos2(face_center.x - side * 0.05, eye_y + side * 0.105),
            egui::pos2(face_center.x - side * 0.025, eye_y + side * 0.117),
            egui::pos2(face_center.x, eye_y + side * 0.12),
            egui::pos2(face_center.x + side * 0.025, eye_y + side * 0.117),
            egui::pos2(face_center.x + side * 0.05, eye_y + side * 0.105),
        ],
        egui::Stroke::new(1.3 * unit, egui::Color32::from_rgb(105, 179, 197)),
    ));
    let chest = center + egui::vec2(0.0, side * 0.205);
    painter.circle_filled(chest, side * 0.042, egui::Color32::from_rgb(7, 26, 40));
    painter.circle_stroke(
        chest,
        side * 0.042,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(49, 146, 174)),
    );
    painter.circle_filled(chest, side * (0.012 + pulse * 0.005), accent);
    for direction in [-1.0, 1.0] {
        painter.line_segment(
            [
                chest + egui::vec2(direction * side * 0.07, side * 0.01),
                chest + egui::vec2(direction * side * 0.11, side * 0.04),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(66, 142, 165)),
        );
    }
    painter.text(
        center + egui::vec2(0.0, side * 0.385),
        egui::Align2::CENTER_CENTER,
        "JARVIS",
        egui::FontId::proportional(14.0 * unit),
        egui::Color32::from_rgb(210, 235, 246),
    );
    if !reduce_motion {
        ctx.request_repaint_after(Duration::from_millis(80));
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

fn memory_endpoint(query: &str) -> String {
    let query = query.trim();
    if query.is_empty() {
        return endpoint("memory").to_owned();
    }
    let encoded = url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>();
    format!("{}?q={encoded}", endpoint("memory"))
}

fn available_pairing_url(current: &str, available: &[String]) -> String {
    if available.iter().any(|url| url == current) {
        current.to_owned()
    } else {
        available.first().cloned().unwrap_or_default()
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
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("JARVIS")
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Library/Application Support/JARVIS")
    }
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

fn safe_oauth_authorize_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn show_notification(item: &Value) {
    let title = item["title"].as_str().unwrap_or("JARVIS");
    let body = item["body"].as_str().unwrap_or("");
    let category = item["category"].as_str().unwrap_or("system");
    let severity = item["severity"].as_str().unwrap_or("info");
    let body = if body.is_empty() {
        format!("{} · {}", category, severity)
    } else {
        format!("{} · {}: {}", category, severity, body)
    };
    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(&body);
    #[cfg(target_os = "windows")]
    if severity == "error" {
        notification.urgency(notify_rust::Urgency::Critical);
    }
    if let Err(error) = notification.show() {
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
    fn dashboard_status_colors_use_service_state_when_aggregate_counts_are_missing() {
        assert_eq!(
            dashboard_core_color(&json!({"core": {"status": "down"}})),
            NEGATIVE
        );
        assert_eq!(
            dashboard_core_color(&json!({
                "core": {"status": "up"},
                "aggregate": {"online": 1, "total": 2}
            })),
            CAUTION
        );
        assert_eq!(
            dashboard_ai_color(&json!({"aggregate": {"ai_online": 0, "total": 0}}), "down"),
            NEGATIVE
        );
        assert_eq!(dashboard_core_color(&json!({})), ACCENT);
    }

    #[test]
    fn dashboard_task_states_have_clear_localized_labels_and_semantic_colors() {
        assert_eq!(dashboard_task_state("succeeded"), ("Completata", POSITIVE));
        assert_eq!(
            dashboard_task_state("waiting_confirmation"),
            ("Richiede conferma", CAUTION)
        );
        assert_eq!(dashboard_task_state("failed"), ("Non riuscita", NEGATIVE));
        assert_eq!(
            dashboard_task_state("future_state"),
            ("Stato non riconosciuto", CAUTION)
        );
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
    fn memory_search_encodes_query_and_omits_blank_filters() {
        assert_eq!(memory_endpoint(""), "/api/memory");
        assert_eq!(memory_endpoint("  "), "/api/memory");
        assert_eq!(
            memory_endpoint("riunioni progetto/2026"),
            "/api/memory?q=riunioni+progetto%2F2026"
        );
    }

    #[test]
    fn pairing_keeps_the_selected_host_or_falls_back_to_an_available_host() {
        let hosts = vec![
            "https://jarvis.example".to_owned(),
            "https://backup.example".to_owned(),
        ];
        assert_eq!(
            available_pairing_url("https://backup.example", &hosts),
            "https://backup.example"
        );
        assert_eq!(
            available_pairing_url("https://removed.example", &hosts),
            "https://jarvis.example"
        );
        assert_eq!(available_pairing_url("https://removed.example", &[]), "");
    }

    #[test]
    fn core_origins_are_normalized_and_reject_paths_or_credentials() {
        assert_eq!(
            normalize_core_origin(" https://jarvis.example/ ").unwrap(),
            "https://jarvis.example"
        );
        assert!(normalize_core_origin("https://user:pass@jarvis.example").is_err());
        assert!(normalize_core_origin("https://jarvis.example/api").is_err());
        assert!(!is_https_url("http://jarvis.example"));
    }

    #[test]
    fn oauth_authorization_urls_require_https_and_no_embedded_credentials() {
        assert!(safe_oauth_authorize_url(
            "https://accounts.example/authorize?state=synthetic"
        ));
        assert!(!safe_oauth_authorize_url(
            "http://accounts.example/authorize"
        ));
        assert!(!safe_oauth_authorize_url(
            "https://user:pass@accounts.example/authorize"
        ));
        assert!(!safe_oauth_authorize_url("javascript:alert(1)"));
    }

    #[test]
    fn dashboard_aggregates_available_cores_and_keeps_offline_results_partial() {
        let mut data = json!({
            "core": {"status": "up"},
            "ai": {"status": "up"},
            "audit": {"chain_valid": true}
        });
        merge_dashboard_summaries(
            &mut data,
            vec![
                json!({
                    "name": "primary.example",
                    "origin": "https://primary.example",
                    "online": true,
                    "ai_online": true,
                    "pending_confirmations": 2,
                    "notifications_unread": 3,
                    "automations": 4,
                    "audit_valid": true,
                    "recent_tasks": [
                        {"created_at": "2026-11-22T10:00:00Z", "backend": "primary.example"}
                    ]
                }),
                json!({
                    "name": "offline.example",
                    "origin": "https://offline.example",
                    "online": false,
                    "ai_online": false,
                    "pending_confirmations": null,
                    "notifications_unread": null,
                    "automations": null,
                    "audit_valid": null,
                    "recent_tasks": null,
                    "error": "Core non raggiungibile o non autorizzato."
                }),
            ],
        );

        assert_eq!(data["aggregate"]["online"], 1);
        assert_eq!(data["aggregate"]["total"], 2);
        assert_eq!(data["core"]["status"], "up");
        assert_eq!(data["aggregate"]["core_label"], "1/2 online");
        assert_eq!(data["pending_confirmations"], 2);
        assert_eq!(data["notifications_unread"], 3);
        assert_eq!(data["automations"], 4);
        assert!(data["audit"]["chain_valid"].is_null());
        assert_eq!(data["recent_tasks"][0]["backend"], "primary.example");
        assert_eq!(data["connected_backends"][1]["online"], false);
    }

    #[test]
    fn backend_summary_exposes_only_selected_health_fields() {
        let health = json!({
            "core": {"status": "up", "secrets_store": "/private/example"},
            "ai": {"status": "up", "error": "private detail"},
            "audit": {"chain_valid": true, "events": 42},
            "web": {"configured_host": "private.example"},
            "pending_confirmations": 1,
            "notifications_unread": 2,
            "automations": 3
        });
        let summary =
            dashboard_backend_summary("example", "https://example.test", &health, vec![], None);
        assert_eq!(summary["online"], true);
        assert_eq!(summary["pending_confirmations"], 1);
        assert!(summary.get("web").is_none());
        assert!(summary["core"].is_null());
        assert!(summary["ai"].is_null());
        assert!(!summary.to_string().contains("private"));
    }

    #[test]
    fn offline_backend_summary_does_not_report_unknown_counts_as_zero() {
        let unavailable = unavailable_backend_summary(
            "offline",
            "https://offline.example",
            "Core non raggiungibile.",
        );
        for field in [
            "pending_confirmations",
            "notifications_unread",
            "automations",
        ] {
            assert!(unavailable[field].is_null(), "{field} should be unknown");
        }
        assert!(unavailable["recent_tasks"].is_null());

        let health = json!({
            "core": {"status": "down"},
            "pending_confirmations": 0,
            "notifications_unread": 0,
            "automations": 0
        });
        let offline =
            dashboard_backend_summary("offline", "https://offline.example", &health, vec![], None);
        assert_eq!(offline["online"], false);
        assert!(offline["pending_confirmations"].is_null());
        assert!(offline["notifications_unread"].is_null());
        assert!(offline["automations"].is_null());
        assert!(offline["recent_tasks"].is_null());

        let healthy = json!({"core": {"status": "up"}});
        let tasks_error = dashboard_backend_summary(
            "partial",
            "https://partial.example",
            &healthy,
            vec![],
            Some("Riepilogo attività non disponibile."),
        );
        assert_eq!(tasks_error["online"], true);
        assert!(tasks_error["recent_tasks"].is_null());
    }

    #[test]
    fn backend_config_round_trips_with_restrictive_unix_permissions() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "jarvis-desktop-backends-{}-{nonce}.json",
            std::process::id()
        ));
        let backends = vec![CoreBackend {
            origin: "https://core.example".into(),
            token: "synthetic-device-token".into(),
        }];
        save_core_backends(&path, &backends).unwrap();
        let loaded = load_core_backends(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].origin, backends[0].origin);
        assert_eq!(loaded[0].token, backends[0].token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_file(path).unwrap();
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
