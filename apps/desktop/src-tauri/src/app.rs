use std::collections::{HashMap, HashSet};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::io::Read;
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

mod users_ui;
use users_ui::UserState;

/// Pagine ordinate per regno (ADR-0232): ogni regno è un gruppo contiguo della barra laterale.
const NAV: [(&str, &str, bool); 25] = [
    // Asgard
    ("dashboard", "Plancia", true),
    ("chat", "Chat", false),
    ("tasks", "Attività", false),
    // Midgard
    ("profile", "Profilo", true),
    ("users", "Utenti", true),
    ("workspace", "Workspace", true),
    // Alfheim
    ("models", "Modelli AI", true),
    ("agents", "Agenti", true),
    ("datasets", "Dataset", true),
    // Vanaheim
    ("skills", "Competenze", true),
    ("plugins", "Pacchetti", true),
    // Jotunheim
    ("missions", "Missioni", true),
    ("plans", "Piani", true),
    ("tools", "Strumenti", true),
    ("updates", "Aggiornamenti", true),
    // Svartalfheim
    ("automations", "Automazioni", true),
    ("connectors", "Connettori", true),
    ("devices", "Dispositivi", true),
    ("system", "Sistema", true),
    // Muspelheim
    ("security", "Sicurezza", true),
    ("permissions", "Permessi", true),
    ("privacy", "Privacy", true),
    // Helheim
    ("memory", "Memoria", true),
    ("research", "Ricerche", true),
    // Niflheim
    ("audit", "Audit", true),
];

/// Sfumatura ai bordi di un elenco che scorre: dal colore del pannello alla trasparenza, in cima o in fondo.
fn scroll_fade(ui: &egui::Ui, view: egui::Rect, fill: egui::Color32, top: bool) {
    let height = 22.0_f32.min(view.height() / 3.0);
    let (solid, clear) = (fill, crate::avatar::alpha(fill, 0.0));
    let rect = if top {
        egui::Rect::from_min_size(view.min, egui::vec2(view.width(), height))
    } else {
        egui::Rect::from_min_size(
            egui::pos2(view.left(), view.bottom() - height),
            egui::vec2(view.width(), height),
        )
    };
    let (upper, lower) = if top { (solid, clear) } else { (clear, solid) };
    let mut mesh = egui::Mesh::default();
    let at = |mesh: &mut egui::Mesh, pos: egui::Pos2, color: egui::Color32| {
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv: egui::epaint::WHITE_UV,
            color,
        });
    };
    at(&mut mesh, rect.left_top(), upper);
    at(&mut mesh, rect.right_top(), upper);
    at(&mut mesh, rect.right_bottom(), lower);
    at(&mut mesh, rect.left_bottom(), lower);
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// Regno della barra laterale di ogni pagina (stessi regni dell'app macOS).
fn nav_group(id: &str) -> &'static str {
    match id {
        "dashboard" | "chat" | "tasks" => "Asgard",
        "profile" | "users" | "workspace" => "Midgard",
        "models" | "agents" | "datasets" => "Alfheim",
        "skills" | "plugins" => "Vanaheim",
        "missions" | "plans" | "tools" | "updates" => "Jotunheim",
        "automations" | "connectors" | "devices" | "system" => "Svartalfheim",
        "security" | "permissions" | "privacy" => "Muspelheim",
        "memory" | "research" => "Helheim",
        _ => "Niflheim",
    }
}

/// Colore del regno, lo stesso dell'albero di Yggdrasill.
fn realm_tint(group: &str) -> egui::Color32 {
    match group {
        "Asgard" => egui::Color32::from_rgb(255, 204, 82),
        "Midgard" => egui::Color32::from_rgb(115, 204, 102),
        "Alfheim" => egui::Color32::from_rgb(184, 153, 255),
        "Vanaheim" => egui::Color32::from_rgb(77, 235, 140),
        "Jotunheim" => egui::Color32::from_rgb(117, 153, 255),
        "Svartalfheim" => egui::Color32::from_rgb(245, 153, 66),
        "Muspelheim" => egui::Color32::from_rgb(255, 87, 46),
        "Helheim" => egui::Color32::from_rgb(133, 184, 189),
        _ => egui::Color32::from_rgb(107, 219, 255),
    }
}

fn mix_color(a: egui::Color32, b: egui::Color32, amount: f32) -> egui::Color32 {
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * amount.clamp(0.0, 1.0)) as u8;
    egui::Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

/// Icona geometrica di una pagina, disegnata con il painter (nessun glifo di font che possa mancare).
fn draw_nav_icon(painter: &egui::Painter, center: egui::Pos2, id: &str, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.4, color);
    let p = |x: f32, y: f32| center + egui::vec2(x, y);
    let rounding = egui::CornerRadius::same(2);
    match id {
        "dashboard" => {
            for (x, y) in [(-6.5, -6.5), (0.5, -6.5), (-6.5, 0.5), (0.5, 0.5)] {
                painter.rect_stroke(
                    egui::Rect::from_min_size(p(x, y), egui::vec2(6.0, 6.0)),
                    rounding,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        "chat" => {
            painter.rect_stroke(
                egui::Rect::from_min_max(p(-7.5, -6.5), p(7.5, 3.5)),
                egui::CornerRadius::same(3),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.add(egui::Shape::line(
                vec![p(-3.0, 3.5), p(-4.5, 7.5), p(1.0, 3.5)],
                stroke,
            ));
        }
        "plans" => {
            let nodes = [p(-6.5, -5.0), p(6.5, -5.0), p(0.0, 6.0)];
            painter.line_segment([nodes[0], nodes[2]], stroke);
            painter.line_segment([nodes[1], nodes[2]], stroke);
            for node in nodes {
                painter.circle_stroke(node, 2.6, stroke);
            }
        }
        "tools" => {
            painter.line_segment([p(-6.5, 6.5), p(2.0, -2.0)], egui::Stroke::new(2.2, color));
            painter.circle_stroke(p(4.5, -4.5), 3.6, stroke);
            painter.line_segment([p(3.0, -6.0), p(6.0, -3.0)], stroke);
        }
        "tasks" => {
            for row in 0..3 {
                let y = -5.5 + row as f32 * 5.5;
                painter.line_segment([p(-1.5, y), p(7.5, y)], stroke);
                painter.circle_filled(p(-6.0, y), 1.4, color);
            }
        }
        "models" => {
            painter.circle_stroke(p(0.0, 0.0), 3.0, stroke);
            for k in 0..6 {
                let angle = k as f32 * std::f32::consts::TAU / 6.0;
                painter.circle_filled(p(angle.cos() * 7.5, angle.sin() * 7.5), 1.3, color);
            }
        }
        "missions" => {
            painter.circle_stroke(p(0.0, 0.0), 7.5, stroke);
            painter.circle_stroke(p(0.0, 0.0), 3.8, stroke);
            painter.circle_filled(p(0.0, 0.0), 1.2, color);
        }
        "automations" => {
            painter.add(egui::Shape::line(
                vec![
                    p(2.5, -8.0),
                    p(-4.5, 1.0),
                    p(0.0, 1.0),
                    p(-2.5, 8.0),
                    p(4.5, -1.5),
                    p(0.0, -1.5),
                    p(2.5, -8.0),
                ],
                stroke,
            ));
        }
        "workspace" => {
            painter.rect_stroke(
                egui::Rect::from_min_max(p(-8.0, -4.5), p(8.0, 6.5)),
                egui::CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([p(-8.0, -4.5), p(-8.0, -7.0)], stroke);
            painter.line_segment([p(-8.0, -7.0), p(-2.0, -7.0)], stroke);
            painter.line_segment([p(-2.0, -7.0), p(0.0, -4.5)], stroke);
        }
        "memory" => {
            painter.circle_stroke(p(-3.0, 0.0), 4.5, stroke);
            painter.circle_stroke(p(3.0, 0.0), 4.5, stroke);
            painter.line_segment([p(0.0, -4.0), p(0.0, 4.0)], stroke);
        }
        "research" => {
            painter.circle_stroke(p(-2.0, -2.0), 5.0, stroke);
            painter.line_segment([p(1.6, 1.6), p(7.0, 7.0)], stroke);
        }
        "privacy" => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(0.0, -8.0),
                    p(6.5, -5.5),
                    p(6.0, 2.0),
                    p(0.0, 8.0),
                    p(-6.0, 2.0),
                    p(-6.5, -5.5),
                ],
                stroke,
            ));
        }
        "skills" => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(0.0, -8.0),
                    p(2.2, -2.2),
                    p(8.0, 0.0),
                    p(2.2, 2.2),
                    p(0.0, 8.0),
                    p(-2.2, 2.2),
                    p(-8.0, 0.0),
                    p(-2.2, -2.2),
                ],
                stroke,
            ));
        }
        "profile" => {
            painter.circle_stroke(p(0.0, -3.5), 3.4, stroke);
            painter.add(egui::Shape::line(
                vec![
                    p(-6.5, 8.0),
                    p(-5.5, 3.0),
                    p(0.0, 1.5),
                    p(5.5, 3.0),
                    p(6.5, 8.0),
                ],
                stroke,
            ));
        }
        "users" => {
            painter.circle_stroke(p(-3.0, -3.5), 2.8, stroke);
            painter.circle_stroke(p(4.5, -2.5), 2.2, stroke);
            painter.line_segment([p(-8.0, 7.0), p(-7.0, 2.5)], stroke);
            painter.line_segment([p(-7.0, 2.5), p(1.0, 2.5)], stroke);
            painter.line_segment([p(1.0, 2.5), p(2.0, 7.0)], stroke);
            painter.line_segment([p(5.0, 2.0), p(8.0, 3.5)], stroke);
        }
        "agents" => {
            painter.circle_stroke(p(0.0, -4.5), 2.8, stroke);
            painter.circle_stroke(p(-5.5, 3.5), 2.4, stroke);
            painter.circle_stroke(p(5.5, 3.5), 2.4, stroke);
        }
        "plugins" => {
            painter.rect_stroke(
                egui::Rect::from_center_size(p(0.0, 1.0), egui::vec2(12.0, 10.0)),
                1.5,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([p(-3.0, -4.0), p(-3.0, -7.5)], stroke);
            painter.line_segment([p(3.0, -4.0), p(3.0, -7.5)], stroke);
        }
        "connectors" => {
            painter.circle_stroke(p(-4.5, 0.0), 4.0, stroke);
            painter.circle_stroke(p(4.5, 0.0), 4.0, stroke);
        }
        "devices" => {
            painter.rect_stroke(
                egui::Rect::from_min_max(p(-8.0, -6.5), p(8.0, 3.5)),
                egui::CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([p(-4.0, 7.0), p(4.0, 7.0)], stroke);
            painter.line_segment([p(0.0, 3.5), p(0.0, 7.0)], stroke);
        }
        "security" => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(0.0, -8.0),
                    p(7.0, -5.0),
                    p(6.0, 3.0),
                    p(0.0, 8.5),
                    p(-6.0, 3.0),
                    p(-7.0, -5.0),
                ],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![p(-3.0, 0.0), p(-0.8, 2.6), p(3.5, -2.5)],
                stroke,
            ));
        }
        "permissions" => {
            painter.circle_stroke(p(-3.5, -3.5), 3.8, stroke);
            painter.line_segment([p(-0.8, -0.8), p(7.5, 7.5)], stroke);
            painter.line_segment([p(4.0, 4.0), p(6.0, 2.0)], stroke);
        }
        "audit" => {
            for row in 0..4 {
                let y = -6.0 + row as f32 * 4.0;
                painter.line_segment([p(-7.0, y), p(7.0 - row as f32 * 1.5, y)], stroke);
            }
        }
        "system" => {
            painter.circle_stroke(p(0.0, 0.0), 3.5, stroke);
            for k in 0..8 {
                let angle = k as f32 * std::f32::consts::TAU / 8.0;
                let direction = egui::vec2(angle.cos(), angle.sin());
                painter.line_segment([center + direction * 5.8, center + direction * 8.0], stroke);
            }
        }
        _ => {
            painter.line_segment([p(0.0, -7.5), p(0.0, 5.5)], stroke);
            painter.add(egui::Shape::line(
                vec![p(-4.5, 1.5), p(0.0, 6.0), p(4.5, 1.5)],
                stroke,
            ));
            painter.line_segment([p(-7.0, 8.0), p(7.0, 8.0)], stroke);
        }
    }
}

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
    CloudTested(Result<Value, String>),
    /// Brano in ascolto su Spotify, l'unico servizio musicale collegato (ADR-0195).
    Spotify(Result<Value, String>),
    /// Stato di n8n in Docker (ADR-0197).
    N8n(Result<Value, String>),
    /// Server MCP autorizzati e, dopo la lettura, gli strumenti di uno di essi (ADR-0199).
    Mcp(Result<Value, String>),
    /// Profili di lavoro (ADR-0185).
    WsProfiles(Result<Value, String>),
    McpTools(String, Result<Value, String>),
    /// Esito dell'avvio dell'aggiornamento interno (ADR-0109): se riuscito l'app si chiude.
    UpdateInstalling(Result<Value, String>),
    /// Personalità corrente di JARVIS (ADR-0120).
    Persona(Result<Value, String>),
    /// Profilo del proprietario (ADR-0133).
    Profile(Result<Value, String>),
    /// Stato della voce di sistema maschile (ADR-0123).
    VoiceStatus(Result<Value, String>),
    /// Chat recenti (ADR-0143).
    Conversations(Result<Value, String>),
    /// Una chat è stata aperta, scelta o eliminata: si ricarica la cronologia.
    ChatChanged(Result<Value, String>),
    /// Lavoro cloud avviato in background (ADR-0144).
    BackgroundStarted(Result<Value, String>),
    /// Lavori in background ancora in corso.
    BackgroundRunning(usize),
    /// Chat esportata in Markdown, pronta da copiare (ADR-0150).
    ChatExported(Result<Value, String>),
    /// Dettaglio di un piano di esecuzione (ADR-0152).
    PlanDetail(Result<Value, String>),
    /// Modelli di piano (ADR-0159).
    PlanTemplates(Result<Value, String>),
    Improvements(Result<Value, String>),
    UserReply(Result<Value, String>),
    UserArea(Result<(Value, Value, Value), String>),
    /// Comando pronunciato dopo «Hey Jarvis» (ADR-0140).
    #[cfg_attr(not(windows), allow(dead_code))]
    WakeCommand(String),
    /// Frammento di testo della risposta in arrivo (ADR-0131).
    ChatDelta(String),
    ChatFinished(Result<Value, String>),
    Notifications(Result<Value, String>),
    /// Il flusso in tempo reale si è interrotto: serve solo a scoprire che l'app è chiusa.
    Ping,
}

struct Connection {
    client: ApiClient,
    core: Option<RunningCore>,
    remote: bool,
    /// Sessione di un utente (ADR-0169): solo chat e profilo.
    user: bool,
}

#[derive(Clone, Debug)]
struct CoreBackend {
    origin: String,
    token: String,
}

#[derive(Clone, Copy)]
enum Mutation {
    Post,
    Put,
    Patch,
    Delete,
}

#[cfg(windows)]
struct JarvisTray {
    _icon: tray_icon::TrayIcon,
    open: tray_icon::menu::MenuId,
    exit: tray_icon::menu::MenuId,
    events: std::sync::mpsc::Receiver<tray_icon::menu::MenuEvent>,
}

#[cfg(windows)]
impl JarvisTray {
    fn new(cc: &eframe::CreationContext<'_>) -> Option<Self> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use tray_icon::menu::{Menu, MenuItem};
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindowAsync, SW_RESTORE, SW_SHOWNA};

        let open = MenuItem::new("Apri JARVIS", true, None);
        let exit = MenuItem::new("Esci definitivamente", true, None);
        let menu = Menu::new();
        menu.append(&open).ok()?;
        menu.append(&exit).ok()?;
        let icon = application_icon();
        let icon = tray_icon::Icon::from_rgba(icon.rgba, icon.width, icon.height).ok()?;
        let tray = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("JARVIS")
            .with_icon(icon)
            .build()
            .ok()?;
        let RawWindowHandle::Win32(window) = cc.window_handle().ok()?.as_raw() else {
            return None;
        };
        let hwnd = window.hwnd.get();
        let open_id = open.id().clone();
        let exit_id = exit.id().clone();
        let ctx = cc.egui_ctx.clone();
        let (sender, events) = std::sync::mpsc::channel();
        tray_icon::menu::MenuEvent::set_event_handler(Some(
            move |event: tray_icon::menu::MenuEvent| {
                let show = if event.id == open_id {
                    SW_RESTORE
                } else if event.id == exit_id {
                    SW_SHOWNA
                } else {
                    return;
                };
                if sender.send(event).is_ok() {
                    // Re-show the native window so eframe can process the command even from the tray.
                    unsafe {
                        ShowWindowAsync(hwnd as _, show);
                    }
                    ctx.request_repaint();
                }
            },
        ));
        Some(Self {
            _icon: tray,
            open: open.id().clone(),
            exit: exit.id().clone(),
            events,
        })
    }
}

pub struct JarvisApp {
    #[cfg(windows)]
    tray: Option<JarvisTray>,
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
    chat_busy: bool,
    chat_cloud_context: bool,
    chat_background: bool,
    tools_query: String,
    tools_category: String,
    plan_selected: Option<String>,
    plan_templates: Vec<Value>,
    plan_templates_loaded: bool,
    improvements: Vec<Value>,
    template_params: std::collections::HashMap<String, String>,
    plan_goal: String,
    nav_scroll_pending: bool,
    /// Accesso e area degli utenti (ADR-0170).
    users: UserState,
    /// Istante dell'ultima operazione riuscita: l'avatar risponde con un'onda verde.
    celebrate_at: f64,
    /// Colore d'accento della personalità dell'utente connesso (RGB).
    accent: Option<[u8; 3]>,
    nav_last_page: String,
    /// Pagina mostrata e istante del cambio: serve alla dissolvenza d'ingresso (ADR-0226).
    page_enter: (String, Instant),
    agent_limits: std::collections::HashMap<String, String>,
    plugin_json: String,
    plan_detail: Option<Value>,
    plan_polled: Instant,
    emergency_active: bool,
    /// Conferma dell'ultima azione riuscita, mostrata per qualche secondo (ADR-0148).
    action_notice: Option<(String, Instant)>,
    background_running: usize,
    background_polled: Instant,
    conversations: Vec<Value>,
    conversations_loaded: bool,
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
    mission_goal: String,
    poll_generation: Arc<std::sync::atomic::AtomicU64>,
    mission_every: String,
    mission_connectors: bool,
    proactive_times: Option<[String; 3]>,
    connector_client_id: String,
    connector_credential: String,
    connector_oauth_notice: Option<String>,
    invite_confirm: bool,
    invite_creating: bool,
    cloud_provider: String,
    cloud_api_key: String,
    cloud_action_busy: bool,
    pending_memory_clean: bool,
    spotify: Option<Result<Value, String>>,
    spotify_busy: bool,
    spotify_asked: Option<std::time::Instant>,
    n8n: Option<Result<Value, String>>,
    n8n_busy: bool,
    n8n_asked: bool,
    n8n_name: String,
    n8n_minutes: u64,
    n8n_kind_audit: bool,
    n8n_pull_confirm: bool,
    n8n_polled: Option<std::time::Instant>,
    mcp: Option<Value>,
    mcp_tools: HashMap<String, Vec<Value>>,
    mcp_token: String,
    mcp_busy: bool,
    mcp_asked: bool,
    ws_profiles: Option<Value>,
    ws_busy: bool,
    ws_asked: bool,
    ws_name: String,
    ws_description: String,
    ws_apps: String,
    ws_dirs: String,
    cloud_action_error: Option<String>,
    cloud_action_notice: Option<String>,
    cloud_remove_confirm: bool,
    github_update_token: String,
    update_remove_confirm: bool,
    update_restarting: bool,
    persona_loaded: bool,
    voice_loaded: bool,
    speak_replies: bool,
    /// Fino a quando JARVIS sta leggendo ad alta voce (stima dalla lunghezza): pilota la bocca dell'avatar.
    speaking_until: Option<Instant>,
    voice_hint: String,
    /// Ascolto «Hey Jarvis» sempre attivo (ADR-0140): impostazione del Core e stato del thread di ascolto.
    wake_listening: bool,
    voice_status_seen: bool,
    wake_shared: Option<Arc<crate::wake::Shared>>,
    persona_style: String,
    persona_address: String,
    persona_frequency: String,
    persona_formal: bool,
    profile_loaded: bool,
    public_host: String,
    profile_doc: Value,
    profile_draft: HashMap<String, String>,
    profile_share: bool,
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
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.spacing.interact_size = egui::vec2(40.0, 38.0);
        // Rifiniture in stile Fluent/macOS (ADR-0101): testo leggermente più grande e gerarchia chiara,
        // finestre e menu con angoli generosi, campi di testo più scuri della superficie, link in tinta.
        style.text_styles = [
            (egui::TextStyle::Heading, egui::FontId::proportional(24.0)),
            (egui::TextStyle::Body, egui::FontId::proportional(15.0)),
            (egui::TextStyle::Button, egui::FontId::proportional(15.0)),
            (egui::TextStyle::Small, egui::FontId::proportional(12.5)),
            (egui::TextStyle::Monospace, egui::FontId::monospace(13.5)),
        ]
        .into();
        style.visuals.window_corner_radius = egui::CornerRadius::same(16);
        style.visuals.menu_corner_radius = egui::CornerRadius::same(12);
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(10);
        style.visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 52, 70));
        style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(10);
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(4, 10, 18);
        style.visuals.faint_bg_color = egui::Color32::from_rgb(9, 21, 34);
        style.visuals.hyperlink_color = ACCENT;
        // Fluidità e profondità (ADR-0224): passaggi di colore un poco più lenti (hover, selezione), barre di
        // scorrimento sovrapposte al contenuto che si allargano solo al passaggio del mouse (niente colonna fissa che
        // sposta il testo), ombre morbide per finestre e menu, bordo sottile per staccarli dallo sfondo.
        style.animation_time = 0.14;
        style.spacing.scroll = egui::style::ScrollStyle::floating();
        let soft_shadow = egui::epaint::Shadow {
            offset: [0, 10],
            blur: 28,
            spread: 0,
            color: egui::Color32::from_black_alpha(110),
        };
        style.visuals.window_shadow = soft_shadow;
        style.visuals.popup_shadow = soft_shadow;
        style.visuals.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(33, 62, 82));
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
            chat_busy: false,
            chat_cloud_context: true,
            chat_background: false,
            tools_query: String::new(),
            tools_category: String::new(),
            plan_selected: None,
            plan_templates: Vec::new(),
            plan_templates_loaded: false,
            improvements: Vec::new(),
            template_params: std::collections::HashMap::new(),
            plan_goal: String::new(),
            nav_scroll_pending: true,
            users: UserState::default(),
            celebrate_at: 0.0,
            accent: None,
            nav_last_page: String::new(),
            page_enter: (String::new(), Instant::now()),
            agent_limits: std::collections::HashMap::new(),
            plugin_json: String::new(),
            plan_detail: None,
            plan_polled: Instant::now(),
            emergency_active: false,
            action_notice: None,
            background_running: 0,
            background_polled: Instant::now(),
            conversations: Vec::new(),
            conversations_loaded: false,
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
            setup_tts: "it_IT-riccardo-x_low".into(),
            connector_template: String::new(),
            mission_goal: String::new(),
            poll_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            mission_every: String::new(),
            mission_connectors: false,
            proactive_times: None,
            connector_client_id: String::new(),
            connector_credential: String::new(),
            connector_oauth_notice: None,
            invite_confirm: true,
            invite_creating: false,
            cloud_provider: "nvidia_nim".into(),
            cloud_api_key: String::new(),
            cloud_action_busy: false,
            pending_memory_clean: false,
            spotify: None,
            spotify_busy: false,
            spotify_asked: None,
            n8n: None,
            n8n_busy: false,
            n8n_asked: false,
            n8n_name: String::new(),
            n8n_minutes: 15,
            n8n_kind_audit: false,
            n8n_pull_confirm: false,
            n8n_polled: None,
            mcp: None,
            mcp_tools: HashMap::new(),
            mcp_token: String::new(),
            mcp_busy: false,
            mcp_asked: false,
            ws_profiles: None,
            ws_busy: false,
            ws_asked: false,
            ws_name: String::new(),
            ws_description: String::new(),
            ws_apps: String::new(),
            ws_dirs: String::new(),
            cloud_action_error: None,
            cloud_action_notice: None,
            cloud_remove_confirm: false,
            github_update_token: String::new(),
            update_remove_confirm: false,
            update_restarting: false,
            #[cfg(windows)]
            tray: JarvisTray::new(cc),
            persona_loaded: false,
            voice_loaded: false,
            speak_replies: false,
            speaking_until: None,
            voice_hint: String::new(),
            wake_listening: true,
            voice_status_seen: false,
            wake_shared: None,
            persona_style: "film".into(),
            persona_address: "Signore".into(),
            persona_frequency: "rare".into(),
            persona_formal: true,
            profile_loaded: false,
            public_host: String::new(),
            profile_doc: Value::Null,
            profile_draft: HashMap::new(),
            profile_share: true,
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
                        self.users.busy = false;
                        self.users.mode = connection.user;
                        if self.remote {
                            self.current = "chat".into();
                        }
                        if self.users.mode {
                            // Area dell'utente: nessun flusso di notifiche né cronologia del proprietario.
                            self.users.area_loaded = false;
                            self.users.synced = false;
                            self.chat.clear();
                        } else {
                            self.start_notification_poll(ctx);
                            self.history_loading = true;
                            self.refresh(ctx);
                        }
                    }
                    Err(error) => {
                        self.pairing = false;
                        if self.users.busy {
                            // Accesso o registrazione non riusciti: si resta nella schermata di accesso.
                            self.users.busy = false;
                            self.pair_error = Some(error);
                        } else {
                            self.error = Some(error);
                        }
                    }
                },
                Event::UserReply(result) => {
                    self.apply_user_reply(result);
                    ctx.request_repaint();
                }
                Event::UserArea(result) => {
                    self.apply_user_area(result);
                    ctx.request_repaint();
                }
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
                Event::Conversations(result) => {
                    if let Ok(doc) = result {
                        self.conversations = doc["items"].as_array().cloned().unwrap_or_default();
                    }
                }
                Event::ChatChanged(result) => {
                    match result {
                        Ok(_) => {
                            self.chat.clear();
                            self.confirmations.clear();
                            self.error = None;
                            self.load_history();
                            self.load_conversations();
                        }
                        Err(error) => self.error = Some(error),
                    }
                    ctx.request_repaint();
                }
                Event::Improvements(result) => {
                    if let Ok(doc) = result {
                        self.improvements = doc["items"].as_array().cloned().unwrap_or_default();
                    }
                    ctx.request_repaint();
                }
                Event::PlanTemplates(result) => {
                    if let Ok(doc) = result {
                        self.plan_templates = doc["items"].as_array().cloned().unwrap_or_default();
                    }
                    ctx.request_repaint();
                }
                Event::PlanDetail(result) => {
                    match result {
                        Ok(doc) => self.plan_detail = Some(doc),
                        Err(error) => self.error = Some(error),
                    }
                    ctx.request_repaint();
                }
                Event::ChatExported(result) => {
                    match result {
                        Ok(doc) => {
                            ctx.copy_text(doc["markdown"].as_str().unwrap_or_default().to_owned());
                            self.action_notice = Some((
                                format!(
                                    "Chat copiata negli appunti in Markdown ({} messaggi).",
                                    doc["messages"].as_u64().unwrap_or(0)
                                ),
                                Instant::now(),
                            ));
                        }
                        Err(error) => self.error = Some(error),
                    }
                    ctx.request_repaint();
                }
                Event::BackgroundRunning(count) => {
                    if self.background_running > 0 && count == 0 {
                        // Finito: la risposta è nella chat, si ricarica la cronologia.
                        self.load_history();
                        self.load_conversations();
                    }
                    self.background_running = count;
                    ctx.request_repaint();
                }
                Event::BackgroundStarted(result) => {
                    match result {
                        Ok(_) => {
                            self.background_running = self.background_running.max(1);
                            self.background_polled = Instant::now() - Duration::from_secs(60);
                            self.chat.push(ChatMessage {
                            role: "assistant",
                            text: "Lavoro avviato in background: continua anche se chiudi o nascondi l'app e ti avviso con una notifica quando ha finito. La risposta resterà in questa chat.".into(),
                            });
                        }
                        Err(error) => self.chat.push(ChatMessage {
                            role: "assistant",
                            text: format!("Errore: {error}"),
                        }),
                    }
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
                        Ok(doc) => {
                            self.error = None;
                            if let (Some(code), Some(expires)) =
                                (doc["code"].as_str(), doc["expires_at"].as_str())
                            {
                                self.users.invite_shown =
                                    Some((code.to_owned(), expires.to_owned()));
                            }
                            self.celebrate_at = ctx.input(|input| input.time);
                            if let Some(message) = doc["message"].as_str() {
                                self.action_notice = Some((message.to_owned(), Instant::now()));
                            }
                        }
                        Err(error) => self.error = Some(error),
                    }
                    self.data = None;
                    self.last_load = Instant::now() - Duration::from_secs(60);
                    self.refresh(ctx);
                }
                Event::VoiceStatus(result) => {
                    if let Ok(doc) = result {
                        self.speak_replies = doc["speak_replies"].as_bool().unwrap_or(false);
                        self.voice_hint = doc["hint"].as_str().unwrap_or_default().to_owned();
                        self.wake_listening = doc["wake_listening"].as_bool().unwrap_or(true);
                        self.voice_status_seen = true;
                    }
                }
                Event::Persona(result) => {
                    self.loading = false;
                    match result {
                        Ok(doc) => {
                            self.persona_style = doc["style"].as_str().unwrap_or("film").to_owned();
                            self.persona_address =
                                doc["address"].as_str().unwrap_or_default().to_owned();
                            self.persona_frequency = doc["address_frequency"]
                                .as_str()
                                .unwrap_or("rare")
                                .to_owned();
                            self.persona_formal = doc["formal"].as_bool().unwrap_or(true);
                            self.persona_loaded = true;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Event::Profile(result) => {
                    self.loading = false;
                    match result {
                        Ok(doc) => {
                            for key in [
                                "role",
                                "company",
                                "sector",
                                "expertise",
                                "interests",
                                "goals",
                                "language",
                                "tone",
                                "level",
                            ] {
                                self.profile_draft.insert(
                                    key.to_string(),
                                    doc[key].as_str().unwrap_or_default().to_owned(),
                                );
                            }
                            self.profile_share = doc["share_with_cloud"].as_bool().unwrap_or(true);
                            self.profile_doc = doc;
                            self.profile_loaded = true;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Event::UpdateInstalling(result) => {
                    self.loading = false;
                    match result {
                        Ok(_) => {
                            self.update_restarting = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(error) => self.error = Some(error),
                    }
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
                            let hint = status["key_hint"].as_str().map(str::to_owned);
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
                                match hint {
                                    Some(hint) => format!("Chiave salvata. {hint} Usa Testa chiave per verificarla."),
                                    None => "Chiave salvata e selezionata per la chat. Usa Testa chiave per verificare il provider.".into(),
                                }
                            });
                        }
                        Err(error) => {
                            self.cloud_action_notice = None;
                            self.cloud_action_error = Some(error);
                        }
                    }
                    ctx.request_repaint();
                }
                Event::WsProfiles(result) => {
                    self.ws_busy = false;
                    match result {
                        Ok(value) => self.ws_profiles = Some(value),
                        Err(error) => self.error = Some(error),
                    }
                    ctx.request_repaint();
                }
                Event::Mcp(result) => {
                    self.mcp_busy = false;
                    match result {
                        Ok(value) => self.mcp = Some(value),
                        Err(error) => self.error = Some(error),
                    }
                    ctx.request_repaint();
                }
                Event::McpTools(name, result) => {
                    self.mcp_busy = false;
                    match result {
                        Ok(value) => {
                            let tools = value["tools"].as_array().cloned().unwrap_or_default();
                            self.mcp_tools.insert(name, tools);
                        }
                        Err(error) => self.error = Some(error),
                    }
                    ctx.request_repaint();
                }
                Event::N8n(result) => {
                    self.n8n_busy = false;
                    if let Err(error) = &result {
                        self.error = Some(error.clone());
                    }
                    if let Ok(status) = result {
                        self.n8n = Some(Ok(status));
                    }
                    ctx.request_repaint();
                }
                Event::Spotify(result) => {
                    self.spotify_busy = false;
                    self.spotify = Some(result);
                    ctx.request_repaint();
                }
                Event::CloudTested(result) => {
                    self.cloud_action_busy = false;
                    match result {
                        Ok(result) => {
                            let model = result["model"].as_str().unwrap_or("modello automatico");
                            let models = result["models_available"]
                                .as_u64()
                                .map(|n| format!(" · credenziali valide, {n} modelli disponibili"))
                                .unwrap_or_default();
                            self.cloud_action_notice =
                                Some(format!("Richiesta di prova riuscita · {model}{models}"));
                        }
                        Err(error) => self.cloud_action_error = Some(error),
                    }
                    ctx.request_repaint();
                }
                Event::WakeCommand(text) => {
                    if !self.chat_busy && !self.history_loading {
                        self.current = "chat".into();
                        self.chat_input = text;
                        self.send_chat();
                    }
                    ctx.request_repaint();
                }
                Event::ChatDelta(piece) => {
                    if self.chat_busy {
                        if let Some(last) = self.chat.last_mut().filter(|m| m.role == "assistant") {
                            last.text.push_str(&piece);
                        }
                    }
                    ctx.request_repaint();
                }
                Event::ChatFinished(Ok(result)) => {
                    let reply = result["reply"].as_str().unwrap_or("").to_string();
                    if self.speak_replies && !self.remote && !reply.is_empty() {
                        let seconds = (reply.chars().count() as f32 / 14.0).clamp(1.5, 45.0);
                        self.speaking_until =
                            Some(Instant::now() + Duration::from_secs_f32(seconds));
                        if let Some(client) = self.client.clone() {
                            let text = reply.clone();
                            std::thread::spawn(move || {
                                let _ = client.post("/api/voice/say", Some(&json!({"text": text})));
                            });
                        }
                    }
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
                    self.chat_busy = false;
                    self.load_history();
                    self.refresh(ctx);
                }
                Event::ChatFinished(Err(error)) => {
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
                Event::Ping => {}
            }
        }
    }

    fn retry_connection(&mut self, ctx: &egui::Context) {
        self.error = None;
        spawn_connection(self.tx.clone());
        ctx.request_repaint();
    }

    /// Notifiche in tempo reale (ADR-0098): il flusso del Core avvisa subito quando ne arriva una e il
    /// client rilegge l'elenco; il polling resta come rete di sicurezza, rallentato mentre il flusso è attivo.
    fn start_notification_poll(&self, ctx: &egui::Context) {
        if self.users.mode {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        use std::sync::atomic::Ordering;
        // Ogni nuova connessione al Core sostituisce le precedenti: i thread vecchi si fermano da soli invece di
        // tenere aperti flussi inutili (il Core ne ammette 16).
        let generation = self.poll_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let current = self.poll_generation.clone();
        let live = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (tx, stream_ctx, stream_client, stream_live, stream_current) = (
            self.tx.clone(),
            ctx.clone(),
            client.clone(),
            live.clone(),
            current.clone(),
        );
        std::thread::spawn(move || {
            let fresh = || stream_current.load(Ordering::SeqCst) == generation;
            let mut failures: u32 = 0;
            while fresh() {
                if let Ok(reader) = stream_client.notification_stream() {
                    failures = 0;
                    stream_live.store(true, Ordering::Relaxed);
                    crate::client::read_events(reader, fresh, |name, _| {
                        if matches!(name, "notification" | "resync")
                            && tx
                                .send(Event::Notifications(
                                    stream_client.get("/api/notifications?limit=20"),
                                ))
                                .is_err()
                        {
                            return false;
                        }
                        stream_ctx.request_repaint();
                        true
                    });
                }
                stream_live.store(false, Ordering::Relaxed);
                if tx.send(Event::Ping).is_err() {
                    break;
                }
                std::thread::sleep(reconnect_delay(failures));
                failures = failures.saturating_add(1);
            }
        });
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            while current.load(Ordering::SeqCst) == generation {
                if tx
                    .send(Event::Notifications(
                        client.get("/api/notifications?limit=20"),
                    ))
                    .is_err()
                {
                    break;
                }
                ctx.request_repaint();
                let wait = if live.load(Ordering::Relaxed) { 30 } else { 5 };
                std::thread::sleep(Duration::from_secs(wait));
            }
        });
    }

    /// Annulla i lavori in background in corso (ADR-0150).
    fn cancel_background(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let jobs = client.get("/api/cloud/jobs").unwrap_or(Value::Null);
            for job in jobs["items"].as_array().into_iter().flatten() {
                if job["status"] == "running" {
                    if let Some(id) = job["id"].as_str() {
                        let _ = client.post(&format!("/api/cloud/jobs/{id}/cancel"), None);
                    }
                }
            }
            let _ = tx.send(Event::BackgroundRunning(0));
        });
    }

    fn export_chat(&self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let id = self
            .conversations
            .iter()
            .find(|c| c["active"] == true)
            .and_then(|c| c["id"].as_str())
            .unwrap_or("main")
            .to_owned();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::ChatExported(
                client.get(&format!("/api/conversations/export?id={id}")),
            ));
        });
    }

    fn load_conversations(&self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Conversations(client.get("/api/conversations")));
        });
    }

    /// Chiede al Core di aprire, scegliere o eliminare una chat e poi ricarica la cronologia.
    fn change_chat(&self, path: &'static str, body: Option<Value>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::ChatChanged(client.post(path, body.as_ref())));
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
        // La chat dell'utente non ha una pagina da caricare: legge la sua cronologia a parte.
        if self.users.mode && self.current == "chat" {
            return;
        }
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

    /// Avvia o ferma l'ascolto «Hey Jarvis» secondo l'impostazione del Core (solo Windows; su macOS è nell'app nativa).
    #[cfg(target_os = "windows")]
    fn sync_wake(&mut self, ctx: &egui::Context) {
        let wanted =
            self.wake_listening && self.voice_status_seen && !self.remote && self.client.is_some();
        if !wanted {
            if let Some(shared) = self.wake_shared.take() {
                shared.request_stop();
            }
            return;
        }
        if self.wake_shared.is_none() {
            let Some(client) = self.client.clone() else {
                return;
            };
            let shared = Arc::new(crate::wake::Shared::default());
            let (tx, ctx) = (self.tx.clone(), ctx.clone());
            crate::mic::start(client, shared.clone(), move |text| {
                let _ = tx.send(Event::WakeCommand(text));
                ctx.request_repaint();
            });
            self.wake_shared = Some(shared);
        }
        if let Some(shared) = &self.wake_shared {
            // Mentre JARVIS elabora o parla non ascolta: la sua voce non deve riattivarlo.
            let speaking = self
                .speaking_until
                .is_some_and(|until| Instant::now() < until + Duration::from_millis(1500));
            shared.set_suspended(self.chat_busy || speaking);
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn sync_wake(&mut self, _ctx: &egui::Context) {}

    /// Aggiorna lo stato dell'arresto di emergenza dai dati della pagina corrente, quando li riporta.
    fn sync_emergency(&mut self) {
        let Some(data) = self.data.as_ref() else {
            return;
        };
        let flag = [
            &data["security"]["emergency_stop"],
            &data["emergency_stop"],
            &data["emergency"],
        ]
        .into_iter()
        .find_map(|v| v.as_bool());
        if let Some(active) = flag {
            self.emergency_active = active;
        }
    }

    /// Finché un lavoro in background è in corso controlla ogni pochi secondi se ha finito.
    fn poll_background(&mut self, ctx: &egui::Context) {
        if self.background_running == 0
            || self.remote
            || self.background_polled.elapsed() < Duration::from_secs(5)
        {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.background_polled = Instant::now();
        let tx = self.tx.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            if let Ok(doc) = client.get("/api/cloud/jobs") {
                let running = doc["items"].as_array().map_or(0, |jobs| {
                    jobs.iter().filter(|j| j["status"] == "running").count()
                });
                let _ = tx.send(Event::BackgroundRunning(running));
                repaint.request_repaint();
            }
        });
        ctx.request_repaint_after(Duration::from_secs(5));
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
        let tx = self.tx.clone();
        let use_context = self.chat_cloud_context && !self.remote;
        if self.chat_background && !self.remote {
            // Lavoro in background (ADR-0144): parte subito e prosegue nel Core anche con l'app in secondo piano.
            self.chat.pop();
            self.chat_busy = false;
            std::thread::spawn(move || {
                let result = client.post(
                    "/api/cloud/background",
                    Some(&json!({"message": message, "provider": "auto", "use_context": use_context})),
                );
                let _ = tx.send(Event::BackgroundStarted(result));
            });
            return;
        }
        std::thread::spawn(move || {
            let deltas = tx.clone();
            let result = client.cloud_chat_stream(&message, use_context, &mut |piece| {
                let _ = deltas.send(Event::ChatDelta(piece.to_string()));
            });
            let _ = tx.send(Event::ChatFinished(result));
        });
    }

    fn wake_status_text(&self) -> String {
        use crate::wake::Phase;
        let Some(shared) = &self.wake_shared else {
            return "Ascolto spento: JARVIS non usa il microfono.".into();
        };
        match shared.phase() {
            Phase::Waiting => "In ascolto: solo «Hey Jarvis» attiva JARVIS; l'audio resta sul computer finché non senti il segnale.".into(),
            Phase::Capturing => "Ti ascolto…".into(),
            Phase::Working => "Elaboro il comando…".into(),
            Phase::Unavailable => shared.detail(),
            Phase::Off => "Avvio dell'ascolto…".into(),
        }
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

    /// Chiamata sui profili di lavoro (`body` assente = sola lettura dell'elenco); rilegge l'elenco.
    fn ws_call(&mut self, ctx: &egui::Context, path: &str, body: Option<Value>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if self.ws_busy {
            return;
        }
        self.ws_busy = true;
        let tx = self.tx.clone();
        let path = path.to_string();
        std::thread::spawn(move || {
            if let Some(body) = &body {
                match client.post(&path, Some(body)) {
                    Ok(done) if path.ends_with("/activate") => {
                        let _ = tx.send(Event::ActionDone(Ok(done)));
                    }
                    Ok(_) => {}
                    Err(error) => {
                        let _ = tx.send(Event::WsProfiles(Err(error)));
                        return;
                    }
                }
            }
            let _ = tx.send(Event::WsProfiles(client.get("/api/workspaces/profiles")));
        });
        ctx.request_repaint();
    }

    /// Profili di lavoro scritti a mano: nome, app da aprire e cartelle della workspace.
    fn show_workspace_profiles(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.ws_asked {
            self.ws_asked = true;
            self.ws_call(ctx, "", None);
        }
        let profiles = self
            .ws_profiles
            .as_ref()
            .and_then(|v| v["items"].as_array())
            .cloned()
            .unwrap_or_default();
        let list = |text: &str| -> Vec<String> {
            text.split([',', '\n'])
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        };
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(29, 49, 65)))
            .corner_radius(16)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.heading("Profili di lavoro");
                ui.label(
                    egui::RichText::new("Crea a mano un ambiente: nome, app da aprire e cartelle della workspace. «Attiva» apre solo le app elencate.")
                        .small()
                        .weak(),
                );
                for profile in &profiles {
                    let name = profile["name"].as_str().unwrap_or("Profilo").to_string();
                    let id = profile["id"].as_str().unwrap_or(&name).to_string();
                    let join = |key: &str| {
                        profile[key]
                            .as_array()
                            .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
                            .unwrap_or_default()
                    };
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.strong(&name);
                        ui.label(egui::RichText::new(format!("{} · {}", join("apps"), join("dirs"))).small().weak());
                        if ui.add_enabled(!self.ws_busy, egui::Button::new("Attiva")).clicked() {
                            self.ws_call(ctx, "/api/workspaces/profiles/activate", Some(json!({"id": id.clone()})));
                        }
                        if ui.button("Modifica").clicked() {
                            self.ws_name = name.clone();
                            self.ws_description = profile["description"].as_str().unwrap_or("").to_string();
                            self.ws_apps = join("apps");
                            self.ws_dirs = join("dirs");
                        }
                        if ui.add_enabled(!self.ws_busy, egui::Button::new("Elimina")).clicked() {
                            self.ws_call(ctx, "/api/workspaces/profiles/remove", Some(json!({"id": id.clone()})));
                        }
                    });
                }
                ui.separator();
                ui.add(egui::TextEdit::singleline(&mut self.ws_name).hint_text("Nome del profilo").desired_width(300.0));
                ui.add(egui::TextEdit::singleline(&mut self.ws_description).hint_text("Descrizione (facoltativa)").desired_width(300.0));
                ui.add(egui::TextEdit::singleline(&mut self.ws_apps).hint_text("App, separate da virgola").desired_width(300.0));
                ui.add(egui::TextEdit::singleline(&mut self.ws_dirs).hint_text("Cartelle, separate da virgola").desired_width(300.0));
                if ui
                    .add_enabled(!self.ws_name.trim().is_empty() && !self.ws_busy, egui::Button::new("Salva profilo"))
                    .clicked()
                {
                    let body = json!({"name": self.ws_name.trim(), "description": self.ws_description.trim(),
                        "apps": list(&self.ws_apps), "dirs": list(&self.ws_dirs)});
                    self.ws_name.clear();
                    self.ws_description.clear();
                    self.ws_apps.clear();
                    self.ws_dirs.clear();
                    self.ws_call(ctx, "/api/workspaces/profiles", Some(body));
                }
            });
        ui.add_space(10.0);
    }

    /// Esegue una chiamata ai server MCP e rilegge l'elenco (`path`, `body`); se `read` è un nome, ne legge poi gli strumenti.
    fn mcp_call(
        &mut self,
        ctx: &egui::Context,
        path: &str,
        body: Option<Value>,
        read: Option<String>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if self.mcp_busy {
            return;
        }
        self.mcp_busy = true;
        let tx = self.tx.clone();
        let path = path.to_string();
        std::thread::spawn(move || {
            let result = if path.is_empty() || (path == "/api/mcp/servers" && body.is_none()) {
                client.get(&path)
            } else {
                client
                    .post(&path, body.as_ref())
                    .and_then(|_| client.get("/api/mcp/servers"))
            };
            let _ = tx.send(Event::Mcp(result));
            if let Some(name) = read {
                let tools = client.post("/api/mcp/servers/test", Some(&json!({"name": name})));
                let _ = tx.send(Event::McpTools(name, tools));
            }
        });
        ctx.request_repaint();
    }

    /// Server MCP (Hugging Face con un gesto): gli strumenti di sola lettura possono girare senza conferma se
    /// approvati qui; gli altri chiedono sempre la conferma del proprietario.
    fn show_mcp(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.mcp_asked {
            self.mcp_asked = true;
            self.mcp_call(ctx, "/api/mcp/servers", None, None);
        }
        let servers = self
            .mcp
            .as_ref()
            .and_then(|v| v["items"].as_array())
            .cloned()
            .unwrap_or_default();
        let has_hf = servers.iter().any(|s| s["preset"] == "huggingface");
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(29, 49, 65)))
            .corner_radius(16)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.heading("Server MCP e Hugging Face");
                ui.label(
                    egui::RichText::new("JARVIS può usare gli strumenti di un server MCP. Quelli di sola lettura possono girare senza conferma se li approvi; gli altri la chiedono sempre.")
                        .small()
                        .weak(),
                );
                if !has_hf {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.mcp_token)
                                .password(true)
                                .hint_text("Token di sola lettura (facoltativo)")
                                .desired_width(260.0),
                        );
                        if ui
                            .add_enabled(!self.mcp_busy, egui::Button::new("Collega Hugging Face"))
                            .clicked()
                        {
                            let mut body = json!({"preset": "huggingface", "auto_suggested": true});
                            let token = std::mem::take(&mut self.mcp_token);
                            if !token.trim().is_empty() {
                                body["token"] = json!(token.trim());
                            }
                            self.mcp_call(ctx, "/api/mcp/servers", Some(body), Some("huggingface".into()));
                        }
                    });
                }
                for server in &servers {
                    let name = server["name"].as_str().unwrap_or("server").to_string();
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.strong(&name);
                        ui.label(egui::RichText::new(server["url"].as_str().unwrap_or("")).small().weak());
                        if ui.add_enabled(!self.mcp_busy, egui::Button::new("Leggi strumenti")).clicked() {
                            let n = name.clone();
                            self.mcp_call(ctx, "", None, Some(n));
                        }
                        if ui.add_enabled(!self.mcp_busy, egui::Button::new("Rimuovi")).clicked() {
                            self.mcp_tools.remove(&name);
                            self.mcp_call(ctx, "/api/mcp/servers/remove", Some(json!({"name": name.clone()})), None);
                        }
                    });
                    let auto: Vec<String> = server["auto"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                        .unwrap_or_default();
                    for tool in self.mcp_tools.get(&name).cloned().unwrap_or_default() {
                        let tool_name = tool["name"].as_str().unwrap_or("?").to_string();
                        ui.horizontal(|ui| {
                            ui.monospace(&tool_name);
                            if tool["read_only"] == true {
                                let mut on = auto.contains(&tool_name);
                                if ui.checkbox(&mut on, "Senza conferma").changed() {
                                    let mut next: Vec<String> = auto.iter().filter(|t| **t != tool_name).cloned().collect();
                                    if on {
                                        next.push(tool_name.clone());
                                    }
                                    next.sort();
                                    self.mcp_call(
                                        ctx,
                                        "/api/mcp/servers/policy",
                                        Some(json!({"name": name.clone(), "auto": next})),
                                        None,
                                    );
                                }
                            } else {
                                ui.colored_label(CAUTION, "chiede conferma");
                            }
                        });
                    }
                }
            });
        ui.add_space(10.0);
    }

    /// Legge lo stato di n8n o gli invia un'azione (`method` `GET`, `POST` o `DELETE`).
    fn n8n_call(
        &mut self,
        ctx: &egui::Context,
        method: &'static str,
        path: String,
        body: Option<Value>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if self.n8n_busy {
            return;
        }
        self.n8n_busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = match method {
                "DELETE" => client.delete(&path),
                "POST" => client.post(&path, body.as_ref()),
                _ => client.get(&path),
            };
            // Dopo un'azione si rilegge lo stato aggiornato.
            let result = result.and_then(|_| client.get("/api/n8n"));
            let _ = tx.send(Event::N8n(result));
        });
        ctx.request_repaint();
    }

    /// n8n in un container Docker isolato (ADR-0197): abilita, avvia, ferma e gestisce le routine.
    fn show_n8n(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.n8n_asked {
            self.n8n_asked = true;
            self.n8n_call(ctx, "GET", "/api/n8n".into(), None);
        }
        let status = match &self.n8n {
            Some(Ok(status)) => status.clone(),
            _ => Value::Null,
        };
        let enabled = status["enabled"].as_bool().unwrap_or(false);
        let running = status["running"].as_bool().unwrap_or(false);
        let docker_ok = status["docker_connected"].as_bool().unwrap_or(true);
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(29, 49, 65)))
            .corner_radius(16)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.heading("n8n con Docker");
                ui.label(
                    egui::RichText::new("Container isolato: senza rete, con soli promemoria e controlli pianificati.")
                        .small()
                        .weak(),
                );
                if enabled && !docker_ok {
                    let hint = status["docker"]["hint"]
                        .as_str()
                        .or_else(|| status["error"].as_str())
                        .unwrap_or("Docker non risponde: avvia Docker Desktop.");
                    ui.colored_label(CAUTION, hint);
                }
                ui.horizontal(|ui| {
                    ui.label(if !enabled {
                        "Disattivato"
                    } else if running {
                        "In esecuzione"
                    } else {
                        "Fermo"
                    });
                    let idle = !self.n8n_busy;
                    if !enabled {
                        if ui.add_enabled(idle, egui::Button::new("Abilita n8n")).clicked() {
                            self.n8n_call(ctx, "POST", "/api/n8n".into(), Some(json!({"enable": true})));
                        }
                    } else if running {
                        if ui.add_enabled(idle, egui::Button::new("Ferma")).clicked() {
                            self.n8n_call(ctx, "POST", "/api/n8n".into(), Some(json!({"running": false})));
                        }
                    } else if status["image_present"] == false || status["pull"]["status"] == "pulling" {
                        if ui
                            .add_enabled(
                                idle && status["pull"]["status"] != "pulling",
                                egui::Button::new("Scarica l'immagine e avvia…"),
                            )
                            .clicked()
                        {
                            self.n8n_pull_confirm = true;
                        }
                    } else if ui
                        .add_enabled(idle && docker_ok, egui::Button::new("Avvia con Docker"))
                        .clicked()
                    {
                        self.n8n_call(ctx, "POST", "/api/n8n".into(), Some(json!({"running": true})));
                    }
                    if ui.add_enabled(idle, egui::Button::new("Aggiorna")).clicked() {
                        self.n8n_call(ctx, "GET", "/api/n8n".into(), None);
                    }
                    if self.n8n_busy {
                        ui.add(egui::Spinner::new());
                    }
                });
                if enabled && running {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.n8n_name)
                                .hint_text("Nome della routine")
                                .desired_width(200.0),
                        );
                        ui.checkbox(&mut self.n8n_kind_audit, "Controllo (invece di promemoria)");
                        ui.add(egui::DragValue::new(&mut self.n8n_minutes).range(1..=59).suffix(" min"));
                        let name = self.n8n_name.trim().to_string();
                        if ui
                            .add_enabled(!name.is_empty() && !self.n8n_busy, egui::Button::new("Crea routine"))
                            .clicked()
                        {
                            let body = json!({"name": name.chars().take(80).collect::<String>(),
                                "kind": if self.n8n_kind_audit { "audit" } else { "reminder" },
                                "minutes": self.n8n_minutes});
                            self.n8n_name.clear();
                            self.n8n_call(ctx, "POST", "/api/n8n/jobs".into(), Some(body));
                        }
                    });
                }
                if enabled && docker_ok && status["image_present"] == false && status["pull"]["status"] != "pulling" {
                    ui.colored_label(
                        CAUTION,
                        format!(
                            "L'immagine di n8n non è sul computer (circa {} MB): serve scaricarla una volta.",
                            status["image_estimate_mb"].as_u64().unwrap_or(0)
                        ),
                    );
                }
                if status["pull"]["status"] == "pulling" {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label(format!(
                            "Download dell'immagine in corso… {} s",
                            status["pull"]["elapsed_s"].as_u64().unwrap_or(0)
                        ));
                    });
                    ctx.request_repaint_after(std::time::Duration::from_secs(3));
                    // Una lettura ogni 3 secondi, non a ogni ridisegno.
                    let due = self
                        .n8n_polled
                        .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(3));
                    if due && !self.n8n_busy {
                        self.n8n_polled = Some(std::time::Instant::now());
                        self.n8n_call(ctx, "GET", "/api/n8n".into(), None);
                    }
                } else if status["pull"]["status"] == "failed" {
                    ui.colored_label(NEGATIVE, status["pull"]["detail"].as_str().unwrap_or("Download non riuscito"));
                }
                let jobs = status["items"].as_array().cloned().unwrap_or_default();
                for job in jobs {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} · ogni {} min",
                            job["name"].as_str().unwrap_or("Routine"),
                            job["minutes"].as_u64().unwrap_or(0)
                        ));
                        if let Some(id) = job["id"].as_str() {
                            if ui.add_enabled(!self.n8n_busy, egui::Button::new("Rimuovi")).clicked() {
                                self.n8n_call(ctx, "DELETE", format!("/api/n8n/jobs/{id}"), None);
                            }
                        }
                    });
                }
            });
        if self.n8n_pull_confirm {
            egui::Window::new("Scaricare l'immagine di n8n?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Docker scaricherà l'immagine di n8n (una sola volta) e poi avvierà il container. Puoi continuare a usare JARVIS durante il download.");
                    ui.horizontal(|ui| {
                        if ui.button("Annulla").clicked() {
                            self.n8n_pull_confirm = false;
                        }
                        if ui.button("Scarica e avvia").clicked() {
                            self.n8n_pull_confirm = false;
                            self.n8n_call(ctx, "POST", "/api/n8n".into(), Some(json!({"pull": true, "running": true})));
                        }
                    });
                });
        }
        ui.add_space(10.0);
    }

    /// Chiede lo stato di Spotify o gli invia un comando (`status`, `playpause`, `next`, `previous`).
    fn spotify_call(&mut self, ctx: &egui::Context, command: &str) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if self.spotify_busy {
            return;
        }
        self.spotify_busy = true;
        self.spotify_asked = Some(std::time::Instant::now());
        let tx = self.tx.clone();
        let body = json!({"command": command});
        std::thread::spawn(move || {
            let _ = tx.send(Event::Spotify(
                client.post("/api/media/spotify", Some(&body)),
            ));
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
            let key = key.trim();
            if key.is_empty()
                || key.len() > 1024
                || key.bytes().any(|byte| !byte.is_ascii_graphic())
            {
                self.cloud_action_error = Some(
                    "Inserisci una chiave API valida, senza spazi o caratteri di controllo.".into(),
                );
                return;
            }
            json!({"provider": provider, "key": key, "use_for_chat": true})
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

    fn test_cloud_key(&mut self, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let provider = self.cloud_provider.clone();
        let tx = self.tx.clone();
        self.cloud_action_busy = true;
        self.cloud_action_notice = None;
        self.cloud_action_error = None;
        std::thread::spawn(move || {
            let _ = tx.send(Event::CloudTested(client.test_cloud_key(&provider)));
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
                Mutation::Put => body
                    .as_ref()
                    .ok_or_else(|| "Corpo della richiesta mancante".to_string())
                    .and_then(|body| client.put(&path, body)),
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
        let mut last_group = "";
        for (id, label, owner_only) in NAV {
            if !self.nav_visible(id, owner_only) {
                continue;
            }
            if !compact {
                let group = nav_group(id);
                if group != last_group {
                    ui.add_space(if last_group.is_empty() { 2.0 } else { 14.0 });
                    ui.horizontal(|ui| {
                        let (dot, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter()
                            .circle_filled(dot.center(), 3.6, realm_tint(group));
                        ui.painter().circle_filled(
                            dot.center(),
                            6.0,
                            crate::avatar::alpha(realm_tint(group), 0.18),
                        );
                        ui.label(
                            egui::RichText::new(group.to_uppercase())
                                .size(11.5)
                                .strong()
                                .color(mix_color(
                                    egui::Color32::from_rgb(102, 130, 152),
                                    realm_tint(group),
                                    0.35,
                                )),
                        );
                    });
                    ui.add_space(3.0);
                    last_group = group;
                }
            }
            let selected = self.current == id;
            let size = if compact {
                egui::vec2(148.0, 40.0)
            } else {
                egui::vec2(ui.available_width(), 44.0)
            };
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            let speed = if self.reduce_motion { 0.0 } else { 0.14 };
            let hover = ctx.animate_bool_with_time(
                egui::Id::unique(("nav-hover", id)),
                response.hovered(),
                speed,
            );
            let active = ctx.animate_bool_with_time(
                egui::Id::unique(("nav-active", id)),
                selected,
                speed * 1.4,
            );
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
            let painter = ui.painter();
            let corner = egui::CornerRadius::same(10);
            if hover > 0.0 && active < 1.0 {
                painter.rect_filled(
                    rect,
                    corner,
                    crate::avatar::alpha(egui::Color32::WHITE, 0.07 * hover * (1.0 - active)),
                );
            }
            if active > 0.0 {
                painter.rect_filled(
                    rect.expand(1.5),
                    egui::CornerRadius::same(11),
                    crate::avatar::alpha(ACCENT, 0.10 * active),
                );
                painter.rect_filled(rect, corner, mix_color(RAISED, SELECTED, active));
                painter.rect_stroke(
                    rect,
                    corner,
                    egui::Stroke::new(1.0, crate::avatar::alpha(ACCENT, 0.45 * active)),
                    egui::StrokeKind::Inside,
                );
                let bar = (rect.height() - 14.0) * active;
                painter.rect_filled(
                    egui::Rect::from_center_size(
                        egui::pos2(rect.left() + 4.0, rect.center().y),
                        egui::vec2(3.0, bar),
                    ),
                    egui::CornerRadius::same(2),
                    ACCENT,
                );
            }
            let ink = mix_color(
                mix_color(
                    egui::Color32::from_rgb(158, 178, 194),
                    egui::Color32::from_rgb(222, 238, 246),
                    hover,
                ),
                egui::Color32::from_rgb(177, 245, 246),
                active,
            );
            // Piastrella dietro l'icona: si accende con il passaggio del puntatore e con la pagina attiva.
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(rect.left() + 25.0, rect.center().y),
                    egui::vec2(30.0, 30.0),
                ),
                egui::CornerRadius::same(9),
                crate::avatar::alpha(ACCENT, 0.05 + 0.07 * hover + 0.12 * active),
            );
            draw_nav_icon(
                painter,
                egui::pos2(rect.left() + 25.0, rect.center().y),
                id,
                ink,
            );
            painter.text(
                egui::pos2(rect.left() + 48.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(15.5),
                ink,
            );
            // La pagina attiva si porta in vista quando cambia da un'altra parte dell'app.
            if selected && self.nav_scroll_pending {
                ui.scroll_to_rect(rect.expand2(egui::vec2(0.0, 18.0)), None);
                self.nav_scroll_pending = false;
            }
            if response.clicked() {
                self.connector_credential.clear();
                self.current = id.into();
                self.data = None;
                self.refresh(ctx);
            }
        }
    }

    /// Marchio in cima alla barra laterale: cerchio con arco che ruota e titolo.
    fn sidebar_brand(&self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let center = egui::pos2(rect.left() + 24.0, rect.center().y);
        let time = if self.reduce_motion {
            0.0
        } else {
            ctx.input(|input| input.time) as f32
        };
        painter.circle_filled(center, 17.0, crate::avatar::alpha(ACCENT, 0.12));
        painter.circle_stroke(
            center,
            17.0,
            egui::Stroke::new(1.2, crate::avatar::alpha(ACCENT, 0.55)),
        );
        painter.add(crate::avatar::arc(
            center,
            17.0,
            time * 90.0,
            time * 90.0 + 80.0,
            egui::Stroke::new(2.2, ACCENT),
        ));
        painter.add(crate::avatar::arc(
            center,
            11.0,
            -time * 140.0,
            -time * 140.0 + 120.0,
            egui::Stroke::new(1.4, crate::avatar::alpha(ACCENT, 0.7)),
        ));
        painter.circle_filled(center, 2.6, egui::Color32::WHITE);
        painter.text(
            egui::pos2(rect.left() + 50.0, rect.center().y - 7.0),
            egui::Align2::LEFT_CENTER,
            "JARVIS",
            egui::FontId::proportional(18.0),
            egui::Color32::from_rgb(226, 242, 250),
        );
        painter.text(
            egui::pos2(rect.left() + 50.0, rect.center().y + 10.0),
            egui::Align2::LEFT_CENTER,
            "AI Assistant",
            egui::FontId::proportional(11.0),
            egui::Color32::from_rgb(112, 140, 162),
        );
        if !self.reduce_motion {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// Scheda di stato in fondo alla barra laterale: punto che pulsa, modalità e onda sonora.
    fn sidebar_status(&self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Frame::new()
            .fill(RAISED)
            .stroke(egui::Stroke::new(1.0, crate::avatar::alpha(ACCENT, 0.18)))
            .corner_radius(12)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let pulse = if self.reduce_motion {
                    0.5
                } else {
                    0.5 + 0.5 * (ctx.input(|input| input.time) as f32 * 2.4).sin()
                };
                ui.horizontal(|ui| {
                    let (dot, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        3.5 + pulse * 2.0,
                        crate::avatar::alpha(POSITIVE, 0.25),
                    );
                    ui.painter().circle_filled(dot.center(), 3.0, POSITIVE);
                    ui.label(
                        egui::RichText::new(if self.users.mode {
                            "JARVIS · il tuo account"
                        } else if self.remote {
                            "JARVIS remoto"
                        } else {
                            "JARVIS Online"
                        })
                        .strong()
                        .size(12.5),
                    );
                });
                ui.label(
                    egui::RichText::new(if self.chat_busy || self.loading {
                        "Modalità: Al lavoro"
                    } else {
                        "Modalità: Operativa"
                    })
                    .size(11.0)
                    .color(egui::Color32::from_rgb(130, 158, 178)),
                );
                ui.add_space(4.0);
                crate::avatar::waveform(
                    ui,
                    ctx,
                    self.reduce_motion,
                    egui::vec2(ui.available_width(), 20.0),
                    self.chat_busy || self.loading,
                );
            });
    }

    fn show_chat(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.users.mode {
            self.show_user_chat(ui, ctx);
            return;
        }
        if !self.remote {
            if !self.conversations_loaded {
                self.conversations_loaded = true;
                self.load_conversations();
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("＋ Nuova chat").clicked() {
                    self.change_chat("/api/conversations", Some(json!({})));
                }
                let active = self
                    .conversations
                    .iter()
                    .find(|c| c["active"] == true)
                    .and_then(|c| c["title"].as_str())
                    .unwrap_or("Chat principale")
                    .to_owned();
                egui::ComboBox::from_id_salt("chat-picker")
                    .selected_text(active)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for chat in self.conversations.clone() {
                            let title = chat["title"].as_str().unwrap_or("Chat");
                            if ui
                                .selectable_label(chat["active"] == true, title)
                                .clicked()
                                && chat["active"] != true
                            {
                                self.change_chat(
                                    "/api/conversations/select",
                                    Some(json!({"id": chat["id"]})),
                                );
                            }
                        }
                    });
                if ui.button("Copia chat").on_hover_text("Copia la chat in Markdown").clicked() {
                    self.export_chat();
                }
                if let Some(id) = self.conversations.iter()
                    .find(|chat| chat["active"] == true)
                    .and_then(|chat| chat["id"].as_str()).map(str::to_owned)
                {
                    let label = if id == "main" { "Svuota chat" } else { "Elimina chat" };
                    if ui.button(label).clicked()
                        && rfd::MessageDialog::new().set_title(label)
                            .set_description("Eliminare i messaggi di questa chat?")
                            .set_level(rfd::MessageLevel::Warning)
                            .set_buttons(rfd::MessageButtons::YesNo)
                            .show() == rfd::MessageDialogResult::Yes
                    {
                        self.change_chat("/api/conversations/delete", Some(json!({"id": id})));
                    }
                }
                if self.background_running > 0
                    && ui
                        .button("Annulla lavoro")
                        .on_hover_text("Ferma il lavoro in background in corso")
                        .clicked()
                {
                    self.cancel_background();
                }
                ui.checkbox(&mut self.chat_background, "In background")
                    .on_hover_text(
                        "JARVIS lavora nel cloud anche se l'app è in secondo piano o chiusa e ti avvisa alla fine.",
                    );
            });
            ui.checkbox(
                &mut self.chat_cloud_context,
                "Ricorda la conversazione recente",
            )
            .on_hover_text("Condivide i turni precedenti con lo stesso provider.");
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
                            let ink = if message.role == "user" {
                                egui::Color32::from_rgb(222, 244, 246)
                            } else {
                                egui::Color32::from_rgb(214, 226, 236)
                            };
                            crate::markdown::show(ui, ctx, &message.text, ink);
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
        if self.emergency_active && !self.remote {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(58, 18, 24))
                .stroke(egui::Stroke::new(1.0, NEGATIVE))
                .corner_radius(12)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(NEGATIVE, "⏻ Arresto di emergenza attivo");
                        ui.label("JARVIS non esegue né modifica nulla.");
                        if ui.button("Disattiva").clicked() {
                            self.act(ctx, "/api/emergency".into(), Some(json!({"active": false})));
                        }
                    });
                });
            ui.add_space(6.0);
        }
        if let Some((message, shown)) = self.action_notice.clone() {
            if shown.elapsed() < Duration::from_secs(7) {
                egui::Frame::new()
                    .fill(RAISED)
                    .stroke(egui::Stroke::new(1.0, crate::avatar::alpha(POSITIVE, 0.55)))
                    .corner_radius(12)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(POSITIVE, "✔");
                            ui.label(message);
                        });
                    });
                ui.add_space(6.0);
                ctx.request_repaint_after(Duration::from_secs(1));
            } else {
                self.action_notice = None;
            }
        }
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
            "missions" => self.show_missions(ui, ctx),
            "plans" => self.show_plans(ui, ctx),
            "tools" => self.show_tools(ui, ctx),
            "agents" => self.show_agents(ui, ctx),
            "profile" => self.show_user_profile(ui, ctx),
            "users" => self.show_users(ui, ctx),
            "plugins" => self.show_plugins(ui, ctx),
            "permissions" => self.show_permissions(ui, ctx),
            "privacy" => self.show_privacy(ui, ctx),
            "tasks" | "research" | "datasets" | "automations" | "skills" | "workspace"
            | "devices" | "security" | "audit" => {
                ui.horizontal(|ui| {
                    ui.heading(title(&self.current));
                    if ui.button("Aggiorna").clicked() {
                        self.data = None;
                        self.refresh(ctx);
                    }
                    if self.current == "security"
                        && !self.emergency_active
                        && ui
                            .add(egui::Button::new(
                                egui::RichText::new("⏻ Arresto di emergenza").color(NEGATIVE),
                            ))
                            .on_hover_text("Ferma subito ogni modifica, i lavori in background e le automazioni")
                            .clicked()
                    {
                        self.act(ctx, "/api/emergency".into(), Some(json!({"active": true})));
                    }
                });
                if self.loading {
                    ui.add(egui::Spinner::new());
                }
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                if self.current == "automations" {
                    self.show_n8n(ui, ctx);
                }
                if self.current == "workspace" {
                    self.show_workspace_profiles(ui, ctx);
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

    /// Centro privacy (ADR-0187, ADR-0210): cosa JARVIS conserva, cosa può uscire verso il cloud, cosa è
    /// autorizzato e la cronologia delle ultime modifiche ai permessi. Legge `/api/privacy/overview`.
    fn show_privacy(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Privacy");
            if ui.button("Aggiorna").clicked() {
                self.data = None;
                self.refresh(ctx);
            }
            if self.loading {
                ui.add(egui::Spinner::new());
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(NEGATIVE, error);
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        let card = |ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(10)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(egui::RichText::new(title).strong());
                    body(ui);
                });
            ui.add_space(6.0);
        };
        let stored = &data["stored_locally"];
        card(ui, "Dati conservati sul computer", &mut |ui| {
            ui.label(format!(
                "Memoria: {} ricordi. {}",
                stored["memory"]["total"].as_u64().unwrap_or(0),
                stored["note"].as_str().unwrap_or("")
            ));
            for entry in stored["data_dir_entries"]
                .as_array()
                .into_iter()
                .flatten()
                .take(8)
            {
                ui.small(format!(
                    "{} · {} KB",
                    entry["name"].as_str().unwrap_or("?"),
                    entry["bytes"].as_u64().unwrap_or(0) / 1024
                ));
            }
        });
        card(ui, "Cosa può uscire verso il cloud", &mut |ui| {
            ui.label(data["cloud"]["note"].as_str().unwrap_or(""));
            let providers = data["cloud"]["providers_with_key"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if providers.is_empty() {
                ui.colored_label(
                    POSITIVE,
                    "Nessun provider cloud ha una chiave: nulla parte dal computer.",
                );
            }
            for provider in providers {
                ui.label(format!(
                    "{}: chiave configurata",
                    provider["id"].as_str().unwrap_or("provider")
                ));
            }
        });
        card(ui, "Cosa è autorizzato", &mut |ui| {
            ui.label(format!(
                "Strumenti disponibili: {}. Autonomia: livello {}; lavori in background {}.",
                data["tools"]["count"].as_u64().unwrap_or(0),
                data["autonomy"]["default"].as_u64().unwrap_or(0),
                if data["autonomy"]["background_allowed"].as_bool() == Some(true) {
                    "consentiti"
                } else {
                    "non consentiti"
                }
            ));
            if let Some(effects) = data["tools"]["by_effects"].as_object() {
                for (effect, count) in effects {
                    ui.small(format!("{effect}: {}", count.as_u64().unwrap_or(0)));
                }
            }
            ui.small(format!(
                "Connettori: {} · Server MCP: {} · Osservazioni di file: {}",
                data["connectors"].as_u64().unwrap_or(0),
                data["mcp_servers"].as_u64().unwrap_or(0),
                data["background"]["file_watches"].as_u64().unwrap_or(0)
            ));
        });
        let history = data["permission_history"]["items"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        card(ui, "Cronologia dei permessi", &mut |ui| {
            if history.is_empty() {
                ui.small("Nessuna modifica registrata.");
            }
            for entry in history.iter().take(10) {
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {}",
                        entry["at"].as_str().unwrap_or("?"),
                        entry["kind"].as_str().unwrap_or("?")
                    ))
                    .small()
                    .strong(),
                );
                for change in entry["changes"].as_array().into_iter().flatten().take(6) {
                    ui.small(format!(
                        "{}: {} → {}",
                        change["field"].as_str().unwrap_or("?"),
                        change["from"],
                        change["to"]
                    ));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Permessi e autonomia").clicked() {
                self.go(ctx, "permissions");
            }
            if ui.button("Memoria").clicked() {
                self.go(ctx, "memory");
            }
            if ui.button("Audit").clicked() {
                self.go(ctx, "audit");
            }
        });
    }

    /// Gestione degli strumenti (ADR-0151): registro con categoria, rischio, salute e regola del proprietario.
    /// Supervisione degli agenti (ADR-0164): pausa e limite orario per ogni specializzazione.
    fn show_agents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Agenti");
            if ui.button("Aggiorna").clicked() {
                self.data = None;
                self.refresh(ctx);
            }
            if self.loading {
                ui.add(egui::Spinner::new());
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(NEGATIVE, error);
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        ui.label(
            "Ogni specializzazione lavora solo sul proprio ambito. Puoi metterla in pausa o darle un limite di \
             lavori all'ora: quando è ferma, la richiesta passa al generalista, sempre sotto le stesse policy e \
             conferme. La supervisione restringe e non allarga.",
        );
        if let Some(conflicts) = data["conflicts"].as_str() {
            ui.label(egui::RichText::new(conflicts).small().weak());
        }
        for agent in data["items"].as_array().into_iter().flatten() {
            let name = agent["agent"].as_str().unwrap_or("").to_owned();
            let enabled = agent["enabled"].as_bool().unwrap_or(true);
            let (state, color) = match agent["state"].as_str() {
                Some("attivo") => ("attivo", POSITIVE),
                Some("in pausa") => ("in pausa", CAUTION),
                _ => ("quota esaurita", NEGATIVE),
            };
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(10)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(&name).strong());
                        ui.colored_label(color, state);
                        ui.small(format!(
                            "{} lavori nell'ultima ora{}",
                            agent["runs_last_hour"].as_u64().unwrap_or(0),
                            agent["max_runs_per_hour"]
                                .as_u64()
                                .map(|m| format!(" su {m}"))
                                .unwrap_or_default()
                        ));
                    });
                    ui.label(
                        egui::RichText::new(agent["role"].as_str().unwrap_or(""))
                            .small()
                            .weak(),
                    );
                    let runs = agent["performance"]["runs"].as_u64().unwrap_or(0);
                    if runs > 0 {
                        ui.small(format!(
                            "{runs} lavori in totale · {}% di successi · {} ms in media",
                            agent["performance"]["success_rate"], agent["performance"]["avg_ms"]
                        ));
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button(if enabled {
                                "Metti in pausa"
                            } else {
                                "Riattiva"
                            })
                            .clicked()
                        {
                            self.act(
                                ctx,
                                "/api/agents/limits".into(),
                                Some(json!({"agent": name, "enabled": !enabled})),
                            );
                        }
                        let text = self.agent_limits.entry(name.clone()).or_default();
                        ui.add(
                            egui::TextEdit::singleline(text)
                                .desired_width(70.0)
                                .hint_text("senza limite"),
                        );
                        if ui.button("Imposta il limite orario").clicked() {
                            let value = text.trim().to_owned();
                            let limit = if value.is_empty() {
                                Some(Value::Null)
                            } else {
                                value
                                    .parse::<u64>()
                                    .ok()
                                    .filter(|n| (1..=500).contains(n))
                                    .map(|n| json!(n))
                            };
                            match limit {
                                Some(limit) => {
                                    text.clear();
                                    self.act(
                                        ctx,
                                        "/api/agents/limits".into(),
                                        Some(json!({"agent": name, "max_runs_per_hour": limit})),
                                    );
                                }
                                None => {
                                    self.error = Some(
                                        "Il limite orario va da 1 a 500 (vuoto per toglierlo)"
                                            .into(),
                                    );
                                }
                            }
                        }
                    });
                });
            ui.add_space(6.0);
        }
    }

    /// Pacchetti di estensione (ADR-0165): competenze e procedure in JSON, senza codice.
    fn show_plugins(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Pacchetti");
            if ui.button("Aggiorna").clicked() {
                self.data = None;
                self.refresh(ctx);
            }
            if self.loading {
                ui.add(egui::Spinner::new());
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(NEGATIVE, error);
        }
        ui.label(
            "Un pacchetto aggiunge competenze e procedure, scritte in JSON. Non contiene codice, non usa la rete e \
             non porta chiavi: ciò che porta resta sotto le tue regole, conferme e audit. Si installa tutto o niente.",
        );
        egui::CollapsingHeader::new("Installa o aggiorna un pacchetto")
            .default_open(false)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.plugin_json)
                        .code_editor()
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .hint_text("{\"contract\": \"jarvis.plugin/1\", \"name\": \"…\", \"version\": \"1.0.0\", …}"),
                );
                let ready = !self.plugin_json.trim().is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new("Installa"))
                    .clicked()
                {
                    match serde_json::from_str::<Value>(&self.plugin_json) {
                        Ok(package) if package.is_object() => {
                            self.plugin_json.clear();
                            self.error = None;
                            self.act(ctx, "/api/plugins".into(), Some(package));
                        }
                        Ok(_) => self.error = Some("Il pacchetto deve essere un oggetto JSON".into()),
                        Err(e) => self.error = Some(format!("JSON non valido: {e}")),
                    }
                }
            });
        let Some(data) = self.data.clone() else {
            return;
        };
        let items = data["items"].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            ui.label("Nessun pacchetto installato.");
        }
        for item in items {
            let name = item["name"].as_str().unwrap_or("").to_owned();
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(10)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(&name).strong());
                        ui.label(
                            egui::RichText::new(format!(
                                "v{}",
                                item["version"].as_str().unwrap_or("?")
                            ))
                            .weak(),
                        );
                        let author = item["author"].as_str().unwrap_or("");
                        if !author.is_empty() {
                            ui.label(egui::RichText::new(author).small().weak());
                        }
                    });
                    ui.label(item["description"].as_str().unwrap_or(""));
                    ui.small(format!(
                        "{} competenze · {} procedure · impronta {}",
                        item["skills"].as_array().map_or(0, Vec::len),
                        item["templates"].as_array().map_or(0, Vec::len),
                        item["digest"]
                            .as_str()
                            .unwrap_or("")
                            .get(..12)
                            .unwrap_or("")
                    ));
                    if ui.small_button("Rimuovi").clicked() {
                        self.act(
                            ctx,
                            "/api/plugins/remove".into(),
                            Some(json!({"name": name})),
                        );
                    }
                });
            ui.add_space(6.0);
        }
    }

    fn show_tools(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Strumenti");
            if ui.button("Aggiorna").clicked() {
                self.data = None;
                self.refresh(ctx);
            }
            if self.loading {
                ui.add(egui::Spinner::new());
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(NEGATIVE, error);
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        ui.label(
            "Ogni strumento dichiara rischio, permessi, piattaforme e dipendenze. Qui li vedi con la loro salute e \
             puoi bloccarli: il blocco restringe sempre, non allarga mai i permessi.",
        );
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.tools_query)
                    .hint_text("Cerca uno strumento")
                    .desired_width(200.0),
            );
            if ui
                .selectable_label(self.tools_category.is_empty(), "Tutte")
                .clicked()
            {
                self.tools_category.clear();
            }
            for category in data["categories"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if ui
                    .selectable_label(self.tools_category == category, category)
                    .clicked()
                {
                    self.tools_category = category.to_owned();
                }
            }
        });
        let query = self.tools_query.trim().to_lowercase();
        let mut shown = 0;
        for tool in data["items"].as_array().into_iter().flatten() {
            let name = tool["name"].as_str().unwrap_or("");
            let category = tool["category"].as_str().unwrap_or("");
            if (!self.tools_category.is_empty() && category != self.tools_category)
                || (!query.is_empty()
                    && !name.to_lowercase().contains(&query)
                    && !tool["summary"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&query))
            {
                continue;
            }
            shown += 1;
            let risk_color = match tool["risk"].as_str() {
                Some("low") => POSITIVE,
                Some("medium") => ACCENT,
                Some("high") => CAUTION,
                _ => NEGATIVE,
            };
            let health = tool["health"]["state"].as_str().unwrap_or("unknown");
            let (health_label, health_color) = match health {
                "healthy" => ("in salute", POSITIVE),
                "degraded" => ("instabile", CAUTION),
                "failing" => ("in errore", NEGATIVE),
                _ => ("senza dati", ACCENT),
            };
            let rule = tool["owner_rule"].as_str();
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(10)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(name).strong());
                        ui.colored_label(
                            risk_color,
                            format!("rischio {}", tool["risk"].as_str().unwrap_or("?")),
                        );
                        ui.label(egui::RichText::new(category).weak());
                        ui.colored_label(health_color, health_label);
                        if tool["offline"] == true {
                            ui.label(egui::RichText::new("offline").small().weak());
                        }
                        match rule {
                            Some("deny") => {
                                ui.colored_label(NEGATIVE, "bloccato");
                            }
                            Some("ask") => {
                                ui.colored_label(CAUTION, "chiede conferma");
                            }
                            _ => {}
                        }
                    });
                    ui.label(
                        egui::RichText::new(tool["summary"].as_str().unwrap_or(""))
                            .small()
                            .weak(),
                    );
                    ui.horizontal_wrapped(|ui| {
                        let runs = tool["health"]["runs"].as_u64().unwrap_or(0);
                        if runs > 0 {
                            ui.small(format!(
                                "{runs} esecuzioni · {} ms in media",
                                tool["health"]["avg_ms"].as_f64().unwrap_or(0.0)
                            ));
                        }
                        let blocked = rule == Some("deny");
                        if ui
                            .small_button(if blocked { "Sblocca" } else { "Blocca" })
                            .clicked()
                        {
                            let value = if blocked { Value::Null } else { json!("deny") };
                            self.mutate(
                                ctx,
                                Mutation::Patch,
                                "/api/permissions".into(),
                                Some(json!({"rules": {name: value}})),
                            );
                        }
                    });
                });
            ui.add_space(4.0);
        }
        ui.small(format!(
            "{shown} di {} strumenti",
            data["total"].as_u64().unwrap_or(0)
        ));
    }

    /// Piani di esecuzione (ADR-0152): attività a passi con dipendenze, conferme, annullamento, rollback e replay.
    fn show_plans(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Piani");
            if ui.button("Aggiorna").clicked() {
                self.data = None;
                self.refresh(ctx);
            }
            if self.loading {
                ui.add(egui::Spinner::new());
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(NEGATIVE, error);
        }
        ui.label(
            "Attività complesse eseguite un passo alla volta, con dipendenze, budget di tempo, conferme del \
             proprietario, annullamento degli effetti e cronologia. Si creano da API o dall'assistente.",
        );
        if !self.plan_templates_loaded {
            self.plan_templates_loaded = true;
            if let Some(client) = self.client.clone() {
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Event::PlanTemplates(client.get("/api/plans/templates")));
                    let _ = tx.send(Event::Improvements(client.get("/api/improvements")));
                });
            }
        }
        self.show_improvements(ui, ctx);
        self.show_plan_proposal(ui, ctx);
        self.show_plan_templates(ui, ctx);
        let Some(data) = self.data.clone() else {
            return;
        };
        let items = data["items"].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            ui.label("Nessun piano ancora: avviane uno da un modello qui sopra.");
        }
        for plan in items {
            let id = plan["id"].as_str().unwrap_or("").to_owned();
            let status = plan["status"].as_str().unwrap_or("");
            let (label, color) = match status {
                "succeeded" => ("completato", POSITIVE),
                "running" => ("in esecuzione", ACCENT),
                "waiting" => ("attende conferma", CAUTION),
                "pending" => ("pronto", ACCENT),
                "interrupted" => ("interrotto", CAUTION),
                "cancelled" => ("annullato", CAUTION),
                "budget_exceeded" => ("budget esaurito", NEGATIVE),
                _ => ("non riuscito", NEGATIVE),
            };
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(10)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(plan["goal"].as_str().unwrap_or("")).strong());
                        ui.colored_label(color, label);
                        ui.small(format!(
                            "{}/{} passi",
                            plan["done"].as_u64().unwrap_or(0),
                            plan["steps"].as_u64().unwrap_or(0)
                        ));
                    });
                    ui.horizontal_wrapped(|ui| {
                        if matches!(status, "pending" | "interrupted" | "waiting")
                            && ui.small_button("Esegui").clicked()
                        {
                            self.act(ctx, format!("/api/plans/{id}/run"), Some(json!({})));
                        }
                        if matches!(status, "pending" | "running" | "waiting" | "interrupted")
                            && ui.small_button("Annulla").clicked()
                        {
                            self.act(ctx, format!("/api/plans/{id}/cancel"), Some(json!({})));
                        }
                        if ui.small_button("Replay").clicked() {
                            self.act(ctx, format!("/api/plans/{id}/replay"), Some(json!({})));
                        }
                        if ui.small_button("Dettagli").clicked() {
                            self.plan_selected = Some(id.clone());
                            self.plan_polled = Instant::now() - Duration::from_secs(60);
                        }
                    });
                });
            ui.add_space(4.0);
        }
        self.show_plan_detail(ui, ctx);
    }

    /// Miglioramenti suggeriti (ADR-0163): JARVIS propone, il proprietario decide. Nulla cambia da solo.
    fn show_improvements(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.improvements.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(format!(
            "Miglioramenti suggeriti ({})",
            self.improvements.len()
        ))
        .default_open(true)
        .show(ui, |ui| {
            for item in self.improvements.clone() {
                let id = item["id"].as_str().unwrap_or("").to_owned();
                ui.label(egui::RichText::new(item["title"].as_str().unwrap_or("")).strong());
                ui.label(egui::RichText::new(item["detail"].as_str().unwrap_or("")).weak());
                ui.horizontal(|ui| {
                    if item["action"].is_object() && ui.button("Salva la procedura").clicked() {
                        self.act(
                            ctx,
                            "/api/improvements/apply".into(),
                            Some(json!({"id": id})),
                        );
                        self.plan_templates_loaded = false;
                        self.improvements.retain(|i| i["id"] != id.as_str());
                    }
                    if ui.small_button("Nascondi").clicked() {
                        self.act(
                            ctx,
                            "/api/improvements/dismiss".into(),
                            Some(json!({"id": id})),
                        );
                        self.improvements.retain(|i| i["id"] != id.as_str());
                    }
                });
                ui.separator();
            }
        });
    }

    /// Piano proposto dal modello (ADR-0160): crea una bozza da rivedere; non la avvia.
    fn show_plan_proposal(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::CollapsingHeader::new("Fai proporre un piano al modello")
            .default_open(false)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.plan_goal)
                        .desired_width(420.0)
                        .char_limit(300)
                        .hint_text("Descrivi l'obiettivo (richiede un modello cloud)"),
                );
                let ready = !self.plan_goal.trim().is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new("Proponi una bozza"))
                    .clicked()
                {
                    let goal = self.plan_goal.trim().to_owned();
                    self.plan_goal.clear();
                    self.act(
                        ctx,
                        "/api/plans/propose".into(),
                        Some(json!({"goal": goal, "create": true})),
                    );
                }
                ui.label(
                    egui::RichText::new("La bozza non parte da sola: la rivedi e poi la avvii.")
                        .weak(),
                );
            });
    }

    /// Procedure pronte (ADR-0159): un gesto crea e avvia il piano, con i parametri richiesti.
    fn show_plan_templates(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.plan_templates.is_empty() {
            return;
        }
        egui::CollapsingHeader::new("Procedure pronte")
            .default_open(true)
            .show(ui, |ui| {
                for template in self.plan_templates.clone() {
                    let name = template["name"].as_str().unwrap_or("").to_owned();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(&name).strong());
                        ui.label(
                            egui::RichText::new(template["description"].as_str().unwrap_or(""))
                                .weak(),
                        );
                    });
                    let mut ready = true;
                    let mut params = serde_json::Map::new();
                    for spec in template["params"].as_array().into_iter().flatten() {
                        let param = spec["name"].as_str().unwrap_or("");
                        let key = format!("{name}.{param}");
                        let text = self.template_params.entry(key).or_default();
                        ui.horizontal(|ui| {
                            ui.label(param);
                            ui.add(
                                egui::TextEdit::singleline(text)
                                    .desired_width(200.0)
                                    .hint_text(spec["default"].as_str().unwrap_or(
                                        if spec["required"] == true {
                                            "obbligatorio"
                                        } else {
                                            ""
                                        },
                                    )),
                            );
                        });
                        if !text.trim().is_empty() {
                            params.insert(param.to_owned(), json!(text.trim()));
                        } else if spec["required"] == true {
                            ready = false;
                        }
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("Crea e avvia"))
                        .clicked()
                    {
                        self.act(
                            ctx,
                            "/api/plans/from-template".into(),
                            Some(json!({"template": name, "params": params, "run": true})),
                        );
                    }
                    ui.separator();
                }
            });
    }

    fn show_plan_detail(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(id) = self.plan_selected.clone() else {
            return;
        };
        let running = self
            .plan_detail
            .as_ref()
            .is_some_and(|d| d["id"] == id.as_str() && d["status"] == "running");
        if (self
            .plan_detail
            .as_ref()
            .is_none_or(|d| d["id"] != id.as_str())
            || running)
            && self.plan_polled.elapsed() >= Duration::from_secs(2)
        {
            if let Some(client) = self.client.clone() {
                self.plan_polled = Instant::now();
                let tx = self.tx.clone();
                let path = format!("/api/plans/{id}");
                std::thread::spawn(move || {
                    let _ = tx.send(Event::PlanDetail(client.get(&path)));
                });
            }
        }
        if running {
            ctx.request_repaint_after(Duration::from_secs(2));
        }
        let Some(detail) = self.plan_detail.clone().filter(|d| d["id"] == id.as_str()) else {
            return;
        };
        ui.add_space(8.0);
        egui::Frame::new()
            .fill(RAISED)
            .corner_radius(12)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.strong(detail["goal"].as_str().unwrap_or("Piano"));
                    if ui.small_button("Chiudi").clicked() {
                        self.plan_selected = None;
                    }
                });
                ui.label(egui::RichText::new("Passi").strong());
                for step in detail["steps"].as_array().into_iter().flatten() {
                    let status = step["status"].as_str().unwrap_or("");
                    let (icon, color) = match status {
                        "done" => ("✔", POSITIVE),
                        "failed" => ("✖", NEGATIVE),
                        "waiting_confirmation" => ("⏸", CAUTION),
                        "rolled_back" => ("↩", ACCENT),
                        "running" => ("▶", ACCENT),
                        "skipped" | "cancelled" => ("–", egui::Color32::GRAY),
                        _ => ("○", egui::Color32::GRAY),
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(color, icon);
                        ui.label(format!(
                            "{} · {}",
                            step["id"].as_str().unwrap_or(""),
                            step["tool"].as_str().unwrap_or("")
                        ));
                        if status == "waiting_confirmation" && ui.small_button("Approva").clicked()
                        {
                            self.act(
                                ctx,
                                format!("/api/plans/{id}/approve"),
                                Some(json!({"steps": [step["id"]]})),
                            );
                        }
                        if let Some(code) = step["error"]["code"].as_str() {
                            ui.colored_label(NEGATIVE, code);
                        }
                    });
                }
                let undoable = detail["steps"].as_array().is_some_and(|s| {
                    s.iter()
                        .any(|s| s["status"] == "done" && s["undo"].is_object())
                });
                if undoable
                    && detail["status"] != "running"
                    && ui
                        .small_button("Annulla gli effetti (con conferma)")
                        .clicked()
                {
                    self.act(
                        ctx,
                        format!("/api/plans/{id}/rollback"),
                        Some(json!({"confirm": true})),
                    );
                }
                if matches!(
                    detail["status"].as_str(),
                    Some("failed" | "cancelled" | "interrupted")
                ) && ui
                    .small_button("Ripianifica (bozza di recupero)")
                    .on_hover_text(
                        "Il modello propone un piano di recupero: resta una bozza da rivedere",
                    )
                    .clicked()
                {
                    self.act(
                        ctx,
                        format!("/api/plans/{id}/replan"),
                        Some(json!({"create": true})),
                    );
                }
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Cronologia").strong());
                for event in detail["events"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .rev()
                    .take(12)
                {
                    ui.small(format!(
                        "{} · {} {}",
                        event["at"].as_str().unwrap_or("").get(11..19).unwrap_or(""),
                        event["kind"].as_str().unwrap_or(""),
                        event["detail"].as_str().unwrap_or("")
                    ));
                }
            });
    }

    /// Missioni autonome e iniziative proattive (ADR-0096, ADR-0097).
    /// Permessi del proprietario (ADR-0116): profilo e regole per strumento. Restringono, non allargano.
    fn show_permissions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(data) = self.data.clone() else {
            ui.label("Caricamento dei permessi…");
            return;
        };
        ui.heading("Permessi di JARVIS");
        ui.label("Decidi cosa JARVIS può fare da solo. Puoi solo restringere: le conferme richieste dal catalogo restano sempre.");
        ui.add_space(8.0);
        let profile = data["profile"].as_str().unwrap_or("standard").to_owned();
        let label = |p: &str| match p {
            "prudent" => "Prudente: conferma ogni modifica",
            "read_only" => "Solo lettura: nessuna modifica",
            _ => "Standard",
        };
        ui.horizontal(|ui| {
            ui.label("Profilo");
            egui::ComboBox::from_id_salt("permission-profile")
                .selected_text(label(&profile))
                .show_ui(ui, |ui| {
                    for option in ["standard", "prudent", "read_only"] {
                        if ui
                            .selectable_label(profile == option, label(option))
                            .clicked()
                            && profile != option
                        {
                            self.mutate(
                                ctx,
                                Mutation::Patch,
                                "/api/permissions".into(),
                                Some(json!({"profile": option})),
                            );
                        }
                    }
                });
        });
        ui.add_space(10.0);
        ui.heading("Regole per strumento");
        ui.label(
            "«Chiedi conferma» richiede il tuo consenso a ogni uso; «Vieta» blocca lo strumento.",
        );
        ui.add_space(4.0);
        let tools = data["tools"].as_array().cloned().unwrap_or_default();
        egui::Grid::new("permission-tools")
            .num_columns(4)
            .striped(true)
            .spacing([14.0, 6.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Strumento").strong());
                ui.label(egui::RichText::new("Effetto").strong());
                ui.label(egui::RichText::new("Rischio").strong());
                ui.label(egui::RichText::new("Regola").strong());
                ui.end_row();
                for tool in &tools {
                    let name = tool["name"].as_str().unwrap_or_default();
                    let explicit = data["rules"][name].as_str().unwrap_or("none");
                    let effective = tool["restriction"].as_str().unwrap_or("none");
                    ui.label(name);
                    ui.label(
                        egui::RichText::new(tool["effects"].as_str().unwrap_or(""))
                            .small()
                            .weak(),
                    );
                    ui.label(
                        egui::RichText::new(tool["risk"].as_str().unwrap_or(""))
                            .small()
                            .weak(),
                    );
                    let shown = match (explicit, effective) {
                        ("deny", _) => "Vieta",
                        ("ask", _) => "Chiedi conferma",
                        (_, "deny") => "Vietato dal profilo",
                        (_, "ask") => "Conferma dal profilo",
                        _ => "Nessuna",
                    };
                    egui::ComboBox::from_id_salt(format!("perm-{name}"))
                        .selected_text(shown)
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for (value, text) in [
                                ("none", "Nessuna"),
                                ("ask", "Chiedi conferma"),
                                ("deny", "Vieta"),
                            ] {
                                if ui.selectable_label(explicit == value, text).clicked()
                                    && explicit != value
                                {
                                    let rule = if value == "none" {
                                        Value::Null
                                    } else {
                                        json!(value)
                                    };
                                    self.mutate(
                                        ctx,
                                        Mutation::Patch,
                                        "/api/permissions".into(),
                                        Some(json!({"rules": {name: rule}})),
                                    );
                                }
                            }
                        });
                    ui.end_row();
                }
            });
        ui.add_space(8.0);
        ui.small("Le modifiche sono registrate nell'audit e valgono dal prossimo comando.");
    }

    fn show_missions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Missioni");
            if ui.button("Aggiorna").clicked() {
                self.data = None;
                self.refresh(ctx);
            }
            if self.loading {
                ui.add(egui::Spinner::new());
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(NEGATIVE, error);
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        ui.label(
            "Obiettivi che JARVIS persegue da solo in background, con strumenti che non chiedono conferma, \
             e un rapporto a ogni esecuzione.",
        );
        if data["cloud"] != true {
            ui.colored_label(
                CAUTION,
                "Nessun modello cloud configurato: le missioni usano solo i comandi locali (Modelli AI).",
            );
        }
        egui::Frame::new()
            .fill(SURFACE)
            .corner_radius(14)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.strong("Nuova missione");
                ui.add(
                    egui::TextEdit::multiline(&mut self.mission_goal)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .hint_text("Es. «Ogni mattina controlla le novità su Rust e riassumile»"),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label("Ripeti ogni (minuti, facoltativo)");
                    ui.add(egui::TextEdit::singleline(&mut self.mission_every).desired_width(70.0));
                    ui.checkbox(&mut self.mission_connectors, "Può usare i connettori");
                });
                let goal = self.mission_goal.trim().to_string();
                let every = self.mission_every.trim().parse::<u64>().ok();
                let invalid_every = !self.mission_every.trim().is_empty() && every.is_none();
                if invalid_every {
                    ui.colored_label(
                        CAUTION,
                        "L'intervallo è un numero di minuti (da 5 a 10080).",
                    );
                }
                let ready = goal.chars().count() >= 8 && !invalid_every;
                if ui
                    .add_enabled(ready, egui::Button::new("Avvia missione"))
                    .clicked()
                {
                    let mut body = json!({"goal": goal, "connectors": self.mission_connectors});
                    if let Some(minutes) = every {
                        body["every_minutes"] = json!(minutes);
                    }
                    self.act(ctx, "/api/missions".into(), Some(body));
                    self.mission_goal.clear();
                    self.mission_every.clear();
                }
            });
        ui.add_space(8.0);
        let items = data["items"].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            ui.label(
                "Nessuna missione. Creane una qui sopra o scrivi in chat «avvia una missione: …».",
            );
        }
        for item in &items {
            self.show_mission(ui, ctx, item);
        }
        ui.add_space(12.0);
        self.show_proactive(ui, ctx, &data["proactive"]);
    }

    fn show_mission(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, item: &Value) {
        let id = item["id"].as_str().unwrap_or_default().to_string();
        let status = item["status"].as_str().unwrap_or_default();
        let (label, color) = match status {
            "active" => ("attiva", POSITIVE),
            "paused" => ("in pausa", CAUTION),
            "completed" => ("completata", ACCENT),
            "failed" => ("interrotta", NEGATIVE),
            _ => ("annullata", egui::Color32::GRAY),
        };
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(color, label);
                ui.strong(item["goal"].as_str().unwrap_or("Missione"));
            });
            let mut meta = vec![
                format!("id {id}"),
                format!("{} esecuzioni su {}", item["runs"], item["max_runs"]),
            ];
            if let Some(minutes) = item["every_minutes"].as_u64() {
                meta.push(format!("ogni {minutes} minuti"));
            }
            if let Some(next) = item["next_run_at"].as_str() {
                meta.push(format!("prossima: {next}"));
            }
            ui.small(meta.join(" · "));
            if let Some(result) = item["result"].as_str() {
                ui.label(result);
            } else if let Some(last) = item["journal"].as_array().and_then(|j| j.last()) {
                ui.label(last["summary"].as_str().unwrap_or_default());
            }
            if let Some(journal) = item["journal"].as_array().filter(|j| j.len() > 1) {
                egui::CollapsingHeader::new(format!("Diario ({} esecuzioni)", journal.len()))
                    .id_salt(format!("mission-journal-{id}"))
                    .show(ui, |ui| {
                        for entry in journal.iter().rev() {
                            ui.small(entry["at"].as_str().unwrap_or_default());
                            ui.label(entry["summary"].as_str().unwrap_or_default());
                        }
                    });
            }
            ui.horizontal_wrapped(|ui| {
                let mut act = |ui: &mut egui::Ui, text: &str, action: &str| {
                    if ui.button(text).clicked() {
                        self.act(ctx, format!("/api/missions/{id}/{action}"), Some(json!({})));
                    }
                };
                match status {
                    "active" => {
                        act(ui, "Esegui ora", "run");
                        act(ui, "Pausa", "pause");
                        act(ui, "Annulla", "cancel");
                    }
                    "paused" => {
                        act(ui, "Riprendi", "resume");
                        act(ui, "Annulla", "cancel");
                    }
                    _ => act(ui, "Elimina", "delete"),
                }
            });
        });
    }

    fn show_proactive(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, settings: &Value) {
        egui::Frame::new()
            .fill(SURFACE)
            .corner_radius(14)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.strong("Iniziative proattive");
                ui.label(
                    "JARVIS si fa avanti da solo: tutto è calcolato sul tuo computer e non esce dal Core. \
                     Durante le ore di silenzio non invia nulla.",
                );
                let toggles = [
                    ("enabled", "Iniziative attive"),
                    ("briefing", "Briefing quotidiano"),
                    ("weekly_briefing", "Briefing settimanale della direzione (lunedì)"),
                    ("disk", "Avvisi sullo spazio disco"),
                    ("confirmations", "Promemoria delle conferme in attesa"),
                    ("workflows", "Automazioni che continuano a fallire"),
                    ("stale_todos", "Cose da fare lasciate indietro"),
                    ("sandbox", "Sandbox quasi piena"),
                ];
                for (key, text) in toggles {
                    let mut value = settings[key].as_bool().unwrap_or(false);
                    if ui.checkbox(&mut value, text).changed() {
                        self.mutate(ctx, Mutation::Patch, "/api/proactive".into(), Some(json!({key: value})));
                    }
                }
                let times = self.proactive_times.get_or_insert_with(|| {
                    ["briefing_time", "quiet_from", "quiet_to"]
                        .map(|k| settings[k].as_str().unwrap_or("08:00").to_string())
                });
                let valid = times.iter().all(|t| {
                    t.len() == 5 && t.as_bytes()[2] == b':' && t[..2].parse::<u8>().is_ok_and(|h| h < 24)
                        && t[3..].parse::<u8>().is_ok_and(|m| m < 60)
                });
                ui.horizontal_wrapped(|ui| {
                    for (index, text) in ["Briefing alle", "Silenzio dalle", "alle"].iter().enumerate() {
                        ui.label(*text);
                        ui.add(egui::TextEdit::singleline(&mut times[index]).desired_width(52.0));
                    }
                });
                let snapshot = times.clone();
                ui.horizontal(|ui| {
                    if ui.add_enabled(valid, egui::Button::new("Salva orari")).clicked() {
                        self.mutate(
                            ctx,
                            Mutation::Patch,
                            "/api/proactive".into(),
                            Some(json!({"briefing_time": snapshot[0], "quiet_from": snapshot[1], "quiet_to": snapshot[2]})),
                        );
                    }
                    if ui.button("Briefing adesso").clicked() {
                        self.act(ctx, "/api/proactive/briefing".into(), Some(json!({})));
                    }
                    if ui.button("Briefing settimanale adesso").clicked() {
                        self.act(ctx, "/api/proactive/weekly".into(), Some(json!({})));
                    }
                });
                if !valid {
                    ui.colored_label(CAUTION, "Usa il formato HH:MM, per esempio 08:00.");
                }
            });
    }

    /// Va a una pagina dall'interno della Plancia.
    fn go(&mut self, ctx: &egui::Context, page: &str) {
        self.current = page.into();
        self.data = None;
        self.refresh(ctx);
    }

    /// Plancia (ADR-0111): intestazione con stato, cose che richiedono attenzione, metriche cliccabili,
    /// Core collegati, attività e notifiche. Nessuna altezza fissa: tutto scorre e si adatta alla larghezza.
    fn show_dashboard(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        let data = self.data.clone().unwrap_or(Value::Null);
        let text = |value: &Value| value.as_str().unwrap_or("—").to_owned();
        let count = |value: &Value| {
            value
                .as_u64()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "—".into())
        };
        let core_status = data["aggregate"]["core_label"]
            .as_str()
            .or_else(|| data["core"]["status"].as_str())
            .unwrap_or("—")
            .to_owned();
        let ai_status = data["aggregate"]["ai_label"]
            .as_str()
            .or_else(|| data["ai"]["status"].as_str())
            .unwrap_or("—")
            .to_owned();
        let confirmations = count(&data["pending_confirmations"]);
        let unread = count(&data["notifications_unread"]);
        let automations = count(&data["automations"]);
        let version = text(&data["core"]["version"]);
        let audit_status = if data.is_null() {
            "—"
        } else if data["aggregate"]["all_online"] == false {
            "Parziale"
        } else {
            match data["audit"]["chain_valid"].as_bool() {
                Some(true) => "Verificato",
                Some(false) => "Da controllare",
                None => "—",
            }
        };
        let core_color = if data.is_null() {
            ACCENT
        } else {
            dashboard_core_color(&data)
        };
        let ai_color = if data.is_null() {
            ACCENT
        } else {
            dashboard_ai_color(&data, &ai_status)
        };
        let audit_color = match audit_status {
            "Verificato" => POSITIVE,
            "Da controllare" => NEGATIVE,
            "Parziale" => CAUTION,
            _ => ACCENT,
        };

        // Cose che richiedono attenzione: (colore, titolo, dettaglio, pagina, etichetta del pulsante).
        let mut attention: Vec<(egui::Color32, String, String, &'static str, &'static str)> =
            Vec::new();
        if data["first_run"] == true {
            attention.push((
                CAUTION,
                "Configurazione iniziale da completare".into(),
                "Imposta modelli, sicurezza e dispositivi.".into(),
                "system",
                "Apri Sistema",
            ));
        }
        if let Some(n) = data["pending_confirmations"].as_u64().filter(|n| *n > 0) {
            attention.push((
                CAUTION,
                format!("{n} azioni attendono la tua conferma"),
                "JARVIS non procede senza il tuo consenso.".into(),
                "tasks",
                "Rivedi",
            ));
        }
        if audit_status == "Da controllare" {
            attention.push((
                NEGATIVE,
                "Il registro di audit non risulta integro".into(),
                "Controlla la catena e la firma del registro.".into(),
                "audit",
                "Apri Audit",
            ));
        }
        if data["aggregate"]["all_online"] == false {
            attention.push((
                CAUTION,
                "Un Core collegato non risponde".into(),
                "Controlla rete e associazione nella sezione Core qui sotto.".into(),
                "devices",
                "Dispositivi",
            ));
        }
        if ai_color == NEGATIVE {
            attention.push((
                NEGATIVE,
                "L'intelligenza artificiale non è disponibile".into(),
                "Verifica la chiave e il modello in Modelli AI.".into(),
                "models",
                "Modelli AI",
            ));
        }
        if data["update_available"] == true {
            attention.push((
                ACCENT,
                "È disponibile una nuova versione".into(),
                "Scaricala e installala senza uscire dall'app.".into(),
                "updates",
                "Aggiorna",
            ));
        }
        if data["notifications_unread"].as_u64().is_some_and(|n| n > 0)
            && data["pending_confirmations"].as_u64() == Some(0)
        {
            attention.push((
                ACCENT,
                format!("{unread} notifiche da leggere"),
                "Le trovi qui sotto, in Notifiche recenti.".into(),
                "dashboard",
                "",
            ));
        }

        // Intestazione.
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_rgb(30, 112, 145),
            ))
            .corner_radius(18)
            .inner_margin(egui::Margin::symmetric(20, 16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let wide = ui.available_width() >= 640.0;
                ui.horizontal(|ui| {
                    if wide {
                        // L'avatar occupa uno spazio fisso a destra: niente sovrapposizioni.
                        let side = 132.0;
                        let text_width = (ui.available_width() - side - 16.0).max(200.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(text_width, side),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                self.dashboard_header_text(
                                    ui,
                                    &attention,
                                    &core_status,
                                    &ai_status,
                                    core_color,
                                    ai_color,
                                    &version,
                                );
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(side, side),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                let mood = crate::avatar::Mood {
                                    thinking: self.chat_busy || self.loading,
                                    talking: self
                                        .speaking_until
                                        .is_some_and(|end| end > Instant::now()),
                                    working: self.background_running > 0,
                                    alert: self.emergency_active || self.error.is_some(),
                                    celebrate: self.celebrate_at,
                                    accent: self.accent,
                                };
                                crate::avatar::draw(ui, ctx, self.reduce_motion, side, mood);
                            },
                        );
                    } else {
                        ui.vertical(|ui| {
                            self.dashboard_header_text(
                                ui,
                                &attention,
                                &core_status,
                                &ai_status,
                                core_color,
                                ai_color,
                                &version,
                            );
                        });
                    }
                });
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    for (label, page) in [
                        ("Nuova chat", "chat"),
                        ("Attività", "tasks"),
                        ("Missioni", "missions"),
                        ("Memoria", "memory"),
                        ("Sistema", "system"),
                    ] {
                        let primary = page == "chat";
                        let button = egui::Button::new(egui::RichText::new(label).strong().color(
                            if primary {
                                ACCENT
                            } else {
                                egui::Color32::from_rgb(210, 224, 235)
                            },
                        ))
                        .fill(if primary { SELECTED } else { RAISED })
                        .corner_radius(10)
                        .min_size(egui::vec2(104.0, 34.0));
                        if ui.add(button).clicked() {
                            if page == "chat" {
                                self.current = "chat".into();
                            } else {
                                self.go(ctx, page);
                            }
                        }
                    }
                });
            });

        // Attenzione.
        ui.add_space(16.0);
        ui.heading("Da guardare");
        ui.add_space(6.0);
        if attention.iter().all(|a| a.4.is_empty()) && attention.is_empty() {
            dashboard_row(
                ui,
                POSITIVE,
                "Nessuna azione richiesta",
                "Servizi attivi e nessuna conferma in sospeso.",
            );
        }
        let mut target: Option<&'static str> = None;
        for (color, title, detail, page, action) in &attention {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2((ui.available_width() - 120.0).max(160.0), 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| dashboard_row(ui, *color, title, detail),
                );
                if !action.is_empty()
                    && ui
                        .add(egui::Button::new(*action).min_size(egui::vec2(104.0, 32.0)))
                        .clicked()
                {
                    target = Some(page);
                }
            });
            ui.add_space(4.0);
        }
        if let Some(page) = target {
            self.go(ctx, page);
        }

        // Metriche.
        ui.add_space(14.0);
        ui.heading("Panoramica");
        ui.add_space(6.0);
        let level = |value: &str| match value {
            "0" => POSITIVE,
            "—" => ACCENT,
            _ => CAUTION,
        };
        let metrics: [(&str, String, String, egui::Color32, &str); 6] = [
            (
                "CORE",
                core_status.clone(),
                format!("Versione {version}"),
                core_color,
                "system",
            ),
            (
                "INTELLIGENZA ARTIFICIALE",
                ai_status.clone(),
                "Stato del servizio".into(),
                ai_color,
                "models",
            ),
            (
                "CONFERME",
                confirmations.clone(),
                "In attesa di te".into(),
                level(&confirmations),
                "tasks",
            ),
            (
                "NOTIFICHE",
                unread.clone(),
                "Da leggere".into(),
                level(&unread),
                "",
            ),
            (
                "AUTOMAZIONI",
                automations.clone(),
                "Configurate".into(),
                ACCENT,
                "automations",
            ),
            (
                "INTEGRITÀ AUDIT",
                audit_status.to_owned(),
                "Registro locale".into(),
                audit_color,
                "audit",
            ),
        ];
        let width = ui.available_width();
        let gap = 10.0;
        let columns = ((width + gap) / (210.0 + gap)).floor().clamp(1.0, 3.0) as usize;
        let card_width = (width - gap * (columns as f32 - 1.0)) / columns as f32;
        let mut go_to: Option<&str> = None;
        for row in metrics.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (label, value, detail, color, page) in row {
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(card_width, 82.0),
                        if page.is_empty() {
                            egui::Sense::hover()
                        } else {
                            egui::Sense::click()
                        },
                    );
                    let hovered = response.hovered() && !page.is_empty();
                    let painter = ui.painter_at(rect);
                    painter.rect(
                        rect,
                        12.0,
                        if hovered { SELECTED } else { RAISED },
                        egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(
                                color.r(),
                                color.g(),
                                color.b(),
                                if hovered { 170 } else { 80 },
                            ),
                        ),
                        egui::StrokeKind::Inside,
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            rect.min + egui::vec2(0.0, 14.0),
                            egui::vec2(3.0, 54.0),
                        ),
                        2.0,
                        *color,
                    );
                    painter.text(
                        rect.min + egui::vec2(16.0, 12.0),
                        egui::Align2::LEFT_TOP,
                        *label,
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_rgb(150, 172, 190),
                    );
                    painter.text(
                        rect.min + egui::vec2(16.0, 30.0),
                        egui::Align2::LEFT_TOP,
                        value,
                        egui::FontId::proportional(22.0),
                        *color,
                    );
                    painter.text(
                        rect.min + egui::vec2(16.0, 60.0),
                        egui::Align2::LEFT_TOP,
                        detail,
                        egui::FontId::proportional(11.5),
                        egui::Color32::from_rgb(130, 150, 168),
                    );
                    if response.clicked() {
                        go_to = Some(page);
                    }
                }
            });
            ui.add_space(gap);
        }
        if let Some(page) = go_to {
            self.go(ctx, page);
        }

        self.show_dashboard_protection(ui, &data["security"]);
        self.show_dashboard_spotify(ui, ctx);
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
        ui.add_space(8.0);
        ui.checkbox(&mut self.reduce_motion, "Riduci animazioni");
    }

    #[allow(clippy::too_many_arguments)]
    fn dashboard_header_text(
        &mut self,
        ui: &mut egui::Ui,
        attention: &[(egui::Color32, String, String, &'static str, &'static str)],
        core_status: &str,
        ai_status: &str,
        core_color: egui::Color32,
        ai_color: egui::Color32,
        version: &str,
    ) {
        ui.label(
            egui::RichText::new("JARVIS · PLANCIA")
                .small()
                .strong()
                .color(ACCENT),
        );
        ui.add_space(4.0);
        let urgent = attention.iter().filter(|a| a.0 != ACCENT).count();
        let headline = match urgent {
            0 => "Tutto in ordine.".to_string(),
            1 => "Una cosa richiede la tua attenzione.".to_string(),
            n => format!("{n} cose richiedono la tua attenzione."),
        };
        ui.label(egui::RichText::new(headline).size(26.0).strong());
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for (label, value, color) in [
                ("Core", core_status, core_color),
                ("AI", ai_status, ai_color),
            ] {
                egui::Frame::new()
                    .fill(RAISED)
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 100),
                    ))
                    .corner_radius(12)
                    .inner_margin(egui::Margin::symmetric(10, 5))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(format!("● {label}  {value}"))
                                .small()
                                .strong()
                                .color(color),
                        );
                    });
            }
            if version != "—" {
                ui.label(egui::RichText::new(format!("v{version}")).small().weak());
            }
        });
    }

    /// Scheda «Protezione» (ADR-0149): punteggio, cifratura a riposo e controlli da sistemare.
    fn show_dashboard_protection(&self, ui: &mut egui::Ui, security: &Value) {
        let Some(score) = security["posture"]["score"].as_u64() else {
            return;
        };
        let color = match score {
            90..=100 => POSITIVE,
            60..=89 => CAUTION,
            _ => NEGATIVE,
        };
        ui.add_space(14.0);
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(1.0, crate::avatar::alpha(color, 0.45)))
            .corner_radius(16)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Protezione");
                    ui.label(
                        egui::RichText::new(format!(
                            "{score}/100 · livello {}",
                            security["posture"]["level"].as_str().unwrap_or("—")
                        ))
                        .strong()
                        .color(color),
                    );
                });
                ui.add(
                    egui::ProgressBar::new(score as f32 / 100.0)
                        .fill(color)
                        .desired_height(8.0),
                );
                ui.add_space(6.0);
                let encryption = &security["encryption"];
                let at_rest = encryption["chat_at_rest"] == true;
                ui.label(
                    egui::RichText::new(if at_rest {
                        format!(
                            "🔒 Cifratura a riposo attiva ({}): {}",
                            encryption["algorithm"].as_str().unwrap_or("AEAD"),
                            encryption["covers"]
                                .as_array()
                                .map(|items| items
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join(", "))
                                .unwrap_or_default()
                        )
                    } else {
                        "🔓 Cifratura a riposo non attiva: manca il portachiavi di sistema".into()
                    })
                    .color(if at_rest { POSITIVE } else { CAUTION }),
                );
                for item in security["posture"]["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let ok = item["ok"] == true;
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            if ok { POSITIVE } else { CAUTION },
                            if ok { "✔" } else { "▲" },
                        );
                        ui.label(item["label"].as_str().unwrap_or(""));
                        if !ok {
                            ui.label(
                                egui::RichText::new(item["advice"].as_str().unwrap_or(""))
                                    .small()
                                    .weak(),
                            );
                        }
                    });
                }
            });
    }

    /// Spotify, l'unico servizio musicale collegato: brano in ascolto e comandi. Lo stato si legge ogni 5 secondi
    /// solo mentre la Plancia è visibile; il Core non avvia Spotify.
    fn show_dashboard_spotify(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let due = self
            .spotify_asked
            .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(5));
        if due && !self.spotify_busy {
            self.spotify_call(ctx, "status");
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(5));
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(29, 49, 65)))
            .corner_radius(16)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.heading("Spotify");
                let (line, usable) = match &self.spotify {
                    None => ("Lettura in corso…".to_string(), false),
                    Some(Err(error)) => (error.clone(), false),
                    Some(Ok(value)) => match value["state"].as_str().unwrap_or("closed") {
                        "closed" => ("Spotify non è aperto: aprilo dal computer.".into(), false),
                        "running" => (
                            "Spotify è aperto ma il brano non è leggibile (finestra nascosta)."
                                .into(),
                            true,
                        ),
                        "stopped" => ("Nessun brano in riproduzione.".into(), true),
                        state => {
                            let title = value["title"].as_str().unwrap_or("");
                            let artist = value["artist"].as_str().unwrap_or("");
                            let label = if state == "playing" {
                                "In riproduzione"
                            } else {
                                "In pausa"
                            };
                            if title.is_empty() {
                                (label.to_string(), true)
                            } else if artist.is_empty() {
                                (format!("{label}: {title}"), true)
                            } else {
                                (format!("{label}: {title} — {artist}"), true)
                            }
                        }
                    },
                };
                ui.label(line);
                ui.horizontal(|ui| {
                    for (label, command) in [("⏮", "previous"), ("⏯", "playpause"), ("⏭", "next")]
                    {
                        if ui
                            .add_enabled(usable && !self.spotify_busy, egui::Button::new(label))
                            .clicked()
                        {
                            self.spotify_call(ctx, command);
                        }
                    }
                });
            });
        ui.add_space(10.0);
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
            if ui
                .add_enabled(!self.loading, egui::Button::new("Pulisci ricordi inutili"))
                .on_hover_text("Elimina gli episodi automatici senza valore (ora, calcoli, saluti); gli altri ricordi non si toccano")
                .clicked()
            {
                self.pending_memory_clean = true;
            }
            if self.loading {
                ui.add(egui::Spinner::new());
            }
        });
        if self.pending_memory_clean {
            egui::Window::new("Pulire i ricordi inutili?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Verranno eliminati gli episodi automatici senza valore (per esempio «che ore sono»). I ricordi fissati e quelli che hai scritto tu restano.");
                    ui.horizontal(|ui| {
                        if ui.button("Annulla").clicked() {
                            self.pending_memory_clean = false;
                        }
                        if ui.button("Elimina").clicked() {
                            self.pending_memory_clean = false;
                            self.act(ctx, "/api/memory/cleanup".into(), Some(json!({"apply": true})));
                        }
                    });
                });
        }
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
        if data["available"] == false {
            ui.label("L'installazione della voce è disattivata su questo Core.");
            return;
        }
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

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn import_google_credentials(&mut self, id: &str, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else {
            return;
        };
        // Se il client Google è già salvato (ADR-0231) non si chiede di nuovo il file JSON: il Core lo ritrova.
        // Chiamata locale e rapida; se non c'è un client salvato risponde con un errore e si apre il selettore.
        self.error = None;
        self.connector_oauth_notice = None;
        // Con Maiusc premuto si sceglie un file nuovo (per sostituire il client salvato).
        let replace = ctx.input(|input| input.modifiers.shift);
        if !replace {
            if let Ok(result) = client.post(
                &format!("/api/connectors/{id}/oauth/import"),
                Some(&json!({})),
            ) {
                let _ = self.tx.send(Event::ConnectorOAuth(Ok(result)));
                ctx.request_repaint();
                return;
            }
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("Seleziona le credenziali OAuth Desktop di Google")
            .add_filter("Credenziali Google", &["json"])
            .pick_file()
        else {
            return;
        };
        self.error = None;
        self.connector_oauth_notice = None;
        self.loading = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let endpoint = format!("/api/connectors/{id}/oauth/import");
        std::thread::spawn(move || {
            const MAX_SIZE: u64 = 16 * 1024;
            let result = (|| {
                let mut bytes = Vec::new();
                std::fs::File::open(path)
                    .and_then(|file| file.take(MAX_SIZE + 1).read_to_end(&mut bytes))
                    .map_err(|_| "Impossibile leggere il file di credenziali.".to_string())?;
                if bytes.len() as u64 > MAX_SIZE {
                    return Err("Il file di credenziali supera 16 KB.".into());
                }
                let file: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| "Il file di credenziali non contiene JSON valido.".to_string())?;
                client.post(&endpoint, Some(&file))
            })();
            let _ = tx.send(Event::ConnectorOAuth(result));
            ctx.request_repaint();
        });
    }

    fn show_connectors(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Connettori");
        self.show_mcp(ui, ctx);
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
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        egui::Frame::new()
            .fill(RAISED)
            .stroke(egui::Stroke::new(1.0, SELECTED))
            .corner_radius(10)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.strong(egui::RichText::new("Google Workspace").color(ACCENT));
                ui.label("Importa il JSON OAuth Desktop una sola volta: resta salvato (cifrato) per Gmail e Calendario, anche dopo aggiornamenti e revoche. Per sostituirlo tieni premuto Maiusc mentre clicchi. Google chiederà il consenso per ciascun servizio.");
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for (id, label) in [("gmail", "Importa per Gmail"), ("google_calendar", "Importa per Calendario")] {
                        if ui.add_enabled(!self.loading, egui::Button::new(label)).clicked() {
                            self.import_google_credentials(id, ctx);
                        }
                    }
                });
            });
        ui.add_space(8.0);
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
                if item["authorized"] != true
                    && ui
                        .add_enabled(!self.loading, egui::Button::new("Rimuovi connettore"))
                        .clicked()
                {
                    self.mutate(ctx, Mutation::Delete, format!("/api/connectors/{id}"), None);
                }
            });
        }
    }

    /// Stile e forma di rivolgersi di JARVIS (ADR-0120).
    fn show_persona(&mut self, ui: &mut egui::Ui) {
        if !self.persona_loaded {
            if let Some(client) = self.client.clone() {
                self.persona_loaded = true;
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Event::Persona(client.get("/api/persona")));
                });
            }
        }
        if !self.voice_loaded && !self.remote {
            if let Some(client) = self.client.clone() {
                self.voice_loaded = true;
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Event::VoiceStatus(client.get("/api/voice/system")));
                });
            }
        }
        if !self.remote {
            ui.heading("Voce");
            ui.label("JARVIS parla solo con una voce maschile (macOS: Luca, Eddy, Reed o Rocko; Windows: Cosimo) o con il modello locale installato.");
            let mut speak = self.speak_replies;
            if ui
                .checkbox(&mut speak, "Leggi le risposte ad alta voce")
                .changed()
            {
                self.speak_replies = speak;
                let ctx = ui.ctx().clone();
                self.mutate(
                    &ctx,
                    Mutation::Patch,
                    "/api/voice/system".into(),
                    Some(json!({"speak_replies": speak})),
                );
            }
            if cfg!(target_os = "windows") {
                let mut listening = self.wake_listening;
                if ui
                    .checkbox(&mut listening, "Ascolta sempre «Hey Jarvis»")
                    .changed()
                {
                    self.wake_listening = listening;
                    let ctx = ui.ctx().clone();
                    self.mutate(
                        &ctx,
                        Mutation::Patch,
                        "/api/voice/system".into(),
                        Some(json!({"wake_listening": listening})),
                    );
                }
                ui.small(egui::RichText::new(self.wake_status_text()).weak());
                if let Some(shared) = &self.wake_shared {
                    if shared.phase() != crate::wake::Phase::Unavailable {
                        ui.add(
                            egui::ProgressBar::new(shared.level())
                                .desired_width(160.0)
                                .desired_height(6.0),
                        );
                    }
                }
            }
            if !self.voice_hint.is_empty() {
                ui.small(egui::RichText::new(self.voice_hint.clone()).weak());
            }
            if ui.button("Fai tacere").clicked() {
                if let Some(client) = self.client.clone() {
                    std::thread::spawn(move || {
                        let _ = client.post("/api/voice/stop", None);
                    });
                }
            }
            ui.separator();
        }
        ui.heading("Personalità");
        ui.label("Come JARVIS ti risponde. Non cambia permessi né conferme.");
        ui.horizontal(|ui| {
            for (value, label) in [("film", "Come nel film"), ("sober", "Sobria")] {
                if ui
                    .selectable_label(self.persona_style == value, label)
                    .clicked()
                    && self.persona_style != value
                {
                    self.persona_style = value.into();
                    self.mutate(
                        &ui.ctx().clone(),
                        Mutation::Patch,
                        "/api/persona".into(),
                        Some(json!({"style": value})),
                    );
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Rivolgiti a me con");
            ui.add(
                egui::TextEdit::singleline(&mut self.persona_address)
                    .hint_text("per esempio Signore, Signora o il tuo nome")
                    .desired_width(240.0),
            );
            if ui.button("Salva").clicked() {
                let address = self.persona_address.trim().to_owned();
                self.mutate(
                    &ui.ctx().clone(),
                    Mutation::Patch,
                    "/api/persona".into(),
                    Some(json!({"address": address})),
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Quanto spesso:");
            for (value, label) in [("never", "Mai"), ("rare", "Di rado"), ("often", "Spesso")] {
                if ui
                    .selectable_label(self.persona_frequency == value, label)
                    .clicked()
                    && self.persona_frequency != value
                {
                    self.persona_frequency = value.into();
                    self.mutate(
                        &ui.ctx().clone(),
                        Mutation::Patch,
                        "/api/persona".into(),
                        Some(json!({"address_frequency": value})),
                    );
                }
            }
            ui.separator();
            for (formal, label) in [(true, "Dammi del lei"), (false, "Dammi del tu")] {
                if ui
                    .selectable_label(self.persona_formal == formal, label)
                    .clicked()
                    && self.persona_formal != formal
                {
                    self.persona_formal = formal;
                    self.mutate(
                        &ui.ctx().clone(),
                        Mutation::Patch,
                        "/api/persona".into(),
                        Some(json!({"formal": formal})),
                    );
                }
            }
        });
        ui.separator();
    }

    /// Profilo del proprietario (ADR-0133): ruolo, settore, tono e livello; niente recapiti.
    fn show_profile(&mut self, ui: &mut egui::Ui) {
        if self.remote {
            return;
        }
        if !self.profile_loaded {
            if let Some(client) = self.client.clone() {
                self.profile_loaded = true;
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Event::Profile(client.get("/api/profile")));
                });
            }
        }
        ui.heading("Il tuo profilo");
        let percent = self.profile_doc["completion_percent"].as_u64().unwrap_or(0);
        ui.add(
            egui::ProgressBar::new(percent as f32 / 100.0).text(format!("{percent}% completato")),
        );
        if let Some(missing) = self.profile_doc["missing"]
            .as_array()
            .filter(|m| !m.is_empty())
        {
            let hints: Vec<&str> = missing.iter().filter_map(Value::as_str).take(3).collect();
            ui.small(
                egui::RichText::new(format!(
                    "Per risposte più mirate aggiungi: {}.",
                    hints.join(", ")
                ))
                .weak(),
            );
        }
        for (key, label) in [
            ("role", "Ruolo"),
            ("company", "Azienda"),
            ("sector", "Settore"),
            ("expertise", "Competenze"),
            ("interests", "Interessi"),
            ("goals", "Obiettivi"),
        ] {
            ui.horizontal(|ui| {
                ui.label(format!("{label:<11}"));
                let value = self.profile_draft.entry(key.to_string()).or_default();
                ui.add(egui::TextEdit::singleline(value).desired_width(320.0));
            });
        }
        for (key, label, options) in [
            (
                "language",
                "Lingua",
                [
                    ("auto", "Automatica"),
                    ("it", "Italiano"),
                    ("en", "Inglese"),
                ]
                .as_slice(),
            ),
            (
                "tone",
                "Tono",
                [
                    ("auto", "Automatico"),
                    ("formale", "Formale"),
                    ("cordiale", "Cordiale"),
                    ("diretto", "Diretto"),
                ]
                .as_slice(),
            ),
            (
                "level",
                "Livello tecnico",
                [
                    ("auto", "Automatico"),
                    ("principiante", "Principiante"),
                    ("intermedio", "Intermedio"),
                    ("esperto", "Esperto"),
                ]
                .as_slice(),
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                for (value, text) in options {
                    let current = self
                        .profile_draft
                        .get(key)
                        .map(String::as_str)
                        .unwrap_or("auto");
                    if ui.selectable_label(current == *value, *text).clicked() {
                        self.profile_draft
                            .insert(key.to_string(), (*value).to_string());
                    }
                }
            });
        }
        ui.checkbox(
            &mut self.profile_share,
            "Usa il profilo nelle richieste ai modelli cloud",
        );
        ui.small(
            egui::RichText::new(
                "Niente email, telefoni o password: JARVIS li rifiuta. Senza l'opzione il profilo resta sul computer.",
            )
            .weak(),
        );
        if ui.button("Salva profilo").clicked() {
            let mut body = serde_json::Map::new();
            for (key, value) in &self.profile_draft {
                body.insert(key.clone(), json!(value));
            }
            body.insert("share_with_cloud".into(), json!(self.profile_share));
            self.profile_loaded = false;
            self.mutate(
                &ui.ctx().clone(),
                Mutation::Patch,
                "/api/profile".into(),
                Some(Value::Object(body)),
            );
        }
        ui.separator();
    }

    fn show_cloud_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.show_persona(ui);
        self.show_profile(ui);
        ui.heading("Collega un provider AI");
        ui.label("Le chiavi vengono salvate nel secret store del Core e non sono mostrate dopo il salvataggio.");
        ui.add_space(8.0);

        let mut provider_changed = false;
        egui::ComboBox::from_id_salt("cloud-provider")
            .selected_text("NVIDIA NIM")
            .show_ui(ui, |ui| {
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

            if ui
                .add_enabled(
                    configured && !self.cloud_action_busy && self.cloud_api_key.trim().is_empty(),
                    egui::Button::new("Testa chiave"),
                )
                .clicked()
            {
                self.test_cloud_key(ctx);
            }
            if configured
                && provider.as_ref().is_some_and(|p| p["preferred"] != true)
                && ui
                    .add_enabled(
                        !self.cloud_action_busy,
                        egui::Button::new("Usa per la chat"),
                    )
                    .clicked()
            {
                self.act(
                    ctx,
                    "/api/cloud".into(),
                    Some(json!({"provider": self.cloud_provider, "use_for_chat": true})),
                );
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
        ui.label("Controllo automatico ogni ora. Scarichi, verifichi e installi la nuova versione da qui, senza passare da GitHub.");
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
        self.show_update_install(ui, ctx);
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

    /// Scarica, verifica e installa l'aggiornamento senza uscire dall'app (ADR-0109).
    fn show_update_install(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(state) = self.data.clone() else {
            return;
        };
        let available = state["update_available"] == true;
        let download = &state["download"];
        let status = download["status"].as_str().unwrap_or("idle");
        if !available && status == "idle" {
            return;
        }
        ui.separator();
        ui.heading("Aggiornamento automatico");
        if download["supported"] != true {
            ui.label("Su questo sistema l'aggiornamento resta manuale: apri la release e scarica il file.");
            return;
        }
        let version = download["version"].as_str().unwrap_or_default();
        match status {
            "downloading" => {
                let bytes = download["bytes"].as_u64().unwrap_or(0) as f32;
                let total = download["total"].as_u64().unwrap_or(0) as f32;
                let fraction = if total > 0.0 {
                    (bytes / total).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .show_percentage()
                        .text(format!(
                            "Scarico {version}: {:.0} / {:.0} MB",
                            bytes / 1.0e6,
                            total / 1.0e6
                        )),
                );
                ctx.request_repaint_after(Duration::from_secs(1));
                if self.last_load.elapsed() > Duration::from_secs(1) && !self.loading {
                    self.refresh(ctx);
                }
            }
            "ready" => {
                ui.colored_label(
                    egui::Color32::LIGHT_GREEN,
                    format!("JARVIS {version} è scaricato e verificato (SHA-256)."),
                );
                ui.label("L'app si chiude, installa la nuova versione e si riapre da sola. I tuoi dati restano dove sono.");
                if self.update_restarting {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label("Installazione in corso…");
                    });
                } else if ui
                    .add_enabled(!self.loading, egui::Button::new("Installa e riavvia"))
                    .clicked()
                {
                    if let Some(client) = self.client.clone() {
                        let tx = self.tx.clone();
                        self.loading = true;
                        std::thread::spawn(move || {
                            let _ = tx.send(Event::UpdateInstalling(
                                client.post("/api/updates/install", None),
                            ));
                        });
                    }
                }
            }
            _ => {
                if status == "failed" {
                    if let Some(error) = download["error"].as_str() {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("Download non riuscito: {error}"),
                        );
                    }
                }
                if available
                    && ui
                        .add_enabled(!self.loading, egui::Button::new("Scarica aggiornamento"))
                        .clicked()
                {
                    self.act(ctx, "/api/updates/download".into(), None);
                }
            }
        }
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
            ui.label("2. Apri il collegamento di associazione (o inserisci indirizzo e codice).");
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
                ui.label("Con Tailscale installato e connesso su questo computer basta un tocco:");
                ui.horizontal(|ui| {
                    if ui.button("Collega con Tailscale").clicked() {
                        self.act(ctx, "/api/tailscale/connect".into(), None);
                    }
                    if ui.button("Diagnostica").clicked() {
                        self.act(ctx, "/api/tailscale/diagnose".into(), None);
                    }
                });
                ui.small(
                    egui::RichText::new(
                        "Rende JARVIS raggiungibile solo dai tuoi dispositivi Tailscale (mai pubblico) e autorizza l'indirizzo da solo.",
                    )
                    .weak(),
                );
                ui.colored_label(
                    egui::Color32::from_rgb(255, 197, 108),
                    "Inserisci qui l'indirizzo HTTPS con cui il Core è raggiungibile (per esempio quello di Tailscale): si salva da solo, senza file né riavvii.",
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.public_host)
                            .hint_text("casa.tailnet.ts.net")
                            .desired_width(260.0),
                    );
                    if ui.button("Aggiungi indirizzo").clicked()
                        && !self.public_host.trim().is_empty()
                    {
                        let host = std::mem::take(&mut self.public_host);
                        self.act(
                            ctx,
                            "/api/devices/access".into(),
                            Some(json!({"add": host.trim()})),
                        );
                    }
                });
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
                    if let Some(link) = invite["link"].as_str() {
                        if ui.button("Copia collegamento (un solo tocco)").clicked() {
                            ctx.copy_text(link.to_owned());
                        }
                        ui.small(
                            egui::RichText::new(
                                "Invialo al telefono e aprilo: indirizzo e codice si compilano da soli.",
                            )
                            .weak(),
                        );
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
            ui.heading("Collega JARVIS");
            self.show_user_auth(ui, ctx);
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
                let _ = std::fs::remove_file(data_dir.join("remote-user-token"));
                ApiClient::device(&base, token.to_string()).map(|client| Connection {
                    client,
                    core: None,
                    remote: true,
                    user: false,
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
        #[cfg(windows)]
        if let Some(tray) = &self.tray {
            for event in tray.events.try_iter() {
                if event.id == tray.exit {
                    self.tray = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                if event.id == tray.open {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
            }
            if !self.update_restarting && ctx.input(|input| input.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }
        self.focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
        self.receive_events(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive_events(&ctx);
        self.sync_wake(&ctx);
        self.poll_background(&ctx);
        self.sync_emergency();
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
            if self.nav_last_page != self.current {
                self.nav_last_page = self.current.clone();
                self.nav_scroll_pending = true;
            }
            // Barra laterale ampia e ridimensionabile (la larghezza scelta resta fra un avvio e l'altro).
            egui::Panel::left("navigation")
                .resizable(true)
                .default_size(256.0)
                .size_range(232.0..=360.0)
                .show(ui, |ui| {
                    let footer_height = 112.0;
                    let navigation_height = (ui.available_height() - footer_height).max(0.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), navigation_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add_space(10.0);
                            self.sidebar_brand(ui, &ctx);
                            ui.add_space(8.0);
                            // Barra di scorrimento piena e ben visibile, con sfumature ai bordi quando c'è altro da vedere.
                            let mut scroll = egui::style::ScrollStyle::solid();
                            scroll.bar_width = 9.0;
                            scroll.handle_min_length = 36.0;
                            scroll.bar_inner_margin = 2.0;
                            ui.spacing_mut().scroll = scroll;
                            let area = egui::ScrollArea::vertical()
                                .id_salt("navigation-list")
                                .auto_shrink([false, false])
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                )
                                .show(ui, |ui| self.navigation(ui, false, &ctx));
                            let view = area.inner_rect;
                            let fade = ui.visuals().panel_fill;
                            let offset = area.state.clamped_offset().y;
                            if offset > 1.0 {
                                scroll_fade(ui, view, fade, true);
                            }
                            if offset + view.height() < area.content_size.y - 1.0 {
                                scroll_fade(ui, view, fade, false);
                            }
                        },
                    );
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        ui.add_space(8.0);
                        self.sidebar_status(ui, &ctx);
                    });
                });
        }
        if self.page_enter.0 != self.current {
            self.page_enter = (self.current.clone(), Instant::now());
        }
        let entering = page_entrance(self.page_enter.1.elapsed());
        if entering < 1.0 {
            ctx.request_repaint();
        }
        egui::CentralPanel::default().show(ui, |ui| {
            // Ingresso della pagina: dissolvenza e un breve scorrimento verso l'alto (≈0,22 s).
            ui.set_opacity(0.2 + 0.8 * entering);
            ui.add_space(12.0 + 14.0 * (1.0 - entering));
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
    // Protezione e cifratura (ADR-0149): solo il proprietario le vede.
    if let Ok(security) = client.get("/api/security") {
        data["security"] = security;
    }
    // Nuova versione disponibile (esito dell'ultimo controllo, senza nuove richieste a GitHub).
    if let Ok(updates) = client.get("/api/updates") {
        data["update_available"] = updates["update_available"].clone();
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

/// Riga informativa con barretta colorata, usata da «Da guardare».
fn dashboard_row(ui: &mut egui::Ui, color: egui::Color32, title: &str, detail: &str) {
    egui::Frame::new()
        .fill(RAISED)
        .stroke(egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 70),
        ))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("●").color(color));
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(title).strong());
                    if !detail.is_empty() {
                        ui.label(egui::RichText::new(detail).small().weak());
                    }
                });
            });
        });
}

fn endpoint(page: &str) -> &'static str {
    match page {
        "dashboard" => "/api/health",
        "tasks" => "/api/tasks?limit=50",
        "automations" => "/api/workflows",
        "memory" => "/api/memory",
        "research" => "/api/research",
        "datasets" => "/api/datasets",
        "privacy" => "/api/privacy/overview",
        "skills" => "/api/skills",
        "missions" => "/api/missions",
        "permissions" => "/api/permissions",
        "plans" => "/api/plans",
        "tools" => "/api/tools/registry",
        "profile" => "/api/me",
        "users" => "/api/users",
        "agents" => "/api/agents/supervisor",
        "plugins" => "/api/plugins",
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
                        user: false,
                    }
                });
            }
        }
        // Sessione di un utente (ADR-0169): si riprende dal file privato scritto all'accesso.
        if let Ok(token) = std::fs::read_to_string(data_dir.join("remote-user-token")) {
            if !token.trim().is_empty() {
                return ApiClient::user(url.as_str(), token.trim().to_string()).map(|client| {
                    Connection {
                        client,
                        core: None,
                        remote: true,
                        user: true,
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
        user: false,
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
    fn agents_and_packages_pages_are_owner_only_and_use_the_supervision_and_plugin_apis() {
        for (page, api, group) in [
            ("agents", "/api/agents/supervisor", "Alfheim"),
            ("plugins", "/api/plugins", "Vanaheim"),
        ] {
            assert_eq!(endpoint(page), api);
            assert_eq!(nav_group(page), group);
            assert!(NAV
                .iter()
                .any(|(id, _, owner_only)| *id == page && *owner_only));
        }
        assert_eq!(title("agents"), "Agenti");
        assert_eq!(title("plugins"), "Pacchetti");
    }

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

/// Avanzamento (0→1, con rallentamento finale) dell'ingresso di una pagina dopo `elapsed`.
fn page_entrance(elapsed: Duration) -> f32 {
    let t = (elapsed.as_secs_f32() / 0.22).clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Attesa prima di riaprire il flusso in tempo reale: 1, 2, 4… fino a 30 s, con ±20 % di casualità. Il primo
/// tentativo dopo una connessione riuscita è rapido; un Core spento non viene martellato.
fn reconnect_delay(failures: u32) -> Duration {
    let base = (1u64 << failures.min(5)).min(30) as f64;
    let jitter = 0.8
        + 0.4
            * f64::from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.subsec_nanos() % 1000),
            )
            / 1000.0;
    Duration::from_secs_f64((base * jitter).max(1.0))
}

#[cfg(test)]
mod reconnect_tests {
    use super::*;

    #[test]
    fn page_entrance_runs_from_zero_to_one_and_stays() {
        assert_eq!(page_entrance(Duration::ZERO), 0.0);
        let mid = page_entrance(Duration::from_millis(110));
        assert!(mid > 0.5 && mid < 1.0, "{mid}");
        assert_eq!(page_entrance(Duration::from_millis(220)), 1.0);
        assert_eq!(page_entrance(Duration::from_secs(5)), 1.0);
    }

    #[test]
    fn delay_grows_then_caps() {
        let secs = |n| reconnect_delay(n).as_secs_f64();
        assert!(secs(0) >= 1.0 && secs(0) <= 1.2);
        assert!(secs(3) >= 6.4 && secs(3) <= 9.6);
        assert!(secs(5) >= 24.0 && secs(5) <= 36.0);
        assert!(secs(40) <= 36.0);
    }
}
