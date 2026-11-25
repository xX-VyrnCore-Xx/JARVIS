//! Interfacce per gli utenti (ADR-0170): accesso e registrazione, chat personale, profilo con la personalità, e la
//! pagina «Utenti» del proprietario. Nessuna logica di sicurezza nel client: password, inviti, limiti e isolamento
//! stanno nel Core; qui si raccolgono i dati, si mostrano gli esiti e si conserva il token di sessione (file privato).

use super::*;

#[derive(Default)]
pub(super) struct UserState {
    /// Sessione utente attiva: l'app mostra solo chat e profilo.
    pub mode: bool,
    /// 0 codice del dispositivo, 1 accedi, 2 registrati.
    pub tab: u8,
    pub username: String,
    pub password: String,
    pub display: String,
    pub invite: String,
    pub busy: bool,
    pub input: String,
    pub sending: bool,
    pub memories: Vec<Value>,
    pub area_loaded: bool,
    // Modulo della personalità, riempito dai dati del Core.
    pub synced: bool,
    pub display_name: String,
    pub address: String,
    pub formal: bool,
    pub style: String,
    pub tone: String,
    pub level: String,
    pub role: String,
    pub interests: String,
    pub goals: String,
    pub pw_current: String,
    pub pw_new: String,
    pub delete_confirm: bool,
    // Pagina del proprietario.
    pub invite_shown: Option<(String, String)>,
    pub delete_pending: Option<String>,
}

fn private_write(path: &std::path::Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// HTTPS, oppure HTTP solo verso questo stesso computer (prove in locale).
pub(super) fn auth_origin_ok(base: &str) -> bool {
    url::Url::parse(base).is_ok_and(|url| {
        url.host_str().is_some()
            && match url.scheme() {
                "https" => true,
                "http" => matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")),
                _ => false,
            }
    })
}

/// Colore d'accento dell'avatar in base al tono scelto dall'utente.
pub(super) fn accent_for(tone: &str) -> Option<[u8; 3]> {
    match tone {
        "formale" => Some([120, 124, 230]),
        "cordiale" => Some([255, 178, 102]),
        "diretto" => Some([64, 224, 170]),
        _ => None,
    }
}

impl JarvisApp {
    /// Voci della barra laterale visibili in questa modalità.
    pub(super) fn nav_visible(&self, id: &str, owner_only: bool) -> bool {
        if self.users.mode {
            return matches!(id, "chat" | "profile");
        }
        id != "profile" && !(self.remote && owner_only)
    }

    // ------------------------------------------------------------------ Accesso

    pub(super) fn show_user_auth(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (index, label) in [
                (0u8, "Codice del dispositivo"),
                (1, "Accedi"),
                (2, "Registrati"),
            ] {
                if ui
                    .selectable_label(self.users.tab == index, label)
                    .clicked()
                {
                    self.users.tab = index;
                    self.pair_error = None;
                }
            }
        });
        ui.add_space(10.0);
        match self.users.tab {
            1 | 2 => {
                let register = self.users.tab == 2;
                ui.label(if register {
                    "Crea il tuo account con l'invito che ti ha dato l'amministratore. JARVIS si adatterà a te."
                } else {
                    "Accedi con il tuo nome utente e la tua password."
                });
                let width = ui.available_width().min(460.0);
                ui.add_sized(
                    [width, 38.0],
                    egui::TextEdit::singleline(&mut self.remote_url)
                        .hint_text("Indirizzo del Core (https://…)"),
                );
                ui.add_sized(
                    [width, 38.0],
                    egui::TextEdit::singleline(&mut self.users.username).hint_text("Nome utente"),
                );
                ui.add_sized(
                    [width, 38.0],
                    egui::TextEdit::singleline(&mut self.users.password)
                        .password(true)
                        .hint_text(if register {
                            "Password (da 10 caratteri)"
                        } else {
                            "Password"
                        }),
                );
                if register {
                    ui.add_sized(
                        [width, 38.0],
                        egui::TextEdit::singleline(&mut self.users.display)
                            .hint_text("Come vuoi essere chiamato"),
                    );
                    ui.add_sized(
                        [width, 38.0],
                        egui::TextEdit::singleline(&mut self.users.invite)
                            .hint_text("Codice di invito"),
                    );
                    ui.label(
                        egui::RichText::new(
                            "I tuoi messaggi vanno al modello cloud scelto dall'amministratore. \
                             Non vedrà la tua password, ma non scrivere segreti nella chat.",
                        )
                        .small()
                        .weak(),
                    );
                }
                if let Some(error) = &self.pair_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                let ready = !self.users.busy
                    && !self.users.username.trim().is_empty()
                    && !self.users.password.is_empty();
                if ui
                    .add_enabled(
                        ready,
                        egui::Button::new(if register { "Crea l'account" } else { "Accedi" }),
                    )
                    .clicked()
                {
                    self.user_authenticate(ctx, register);
                }
            }
            _ => {
                ui.label(
                    "Inserisci l’origine HTTPS del Core e il codice temporaneo creato in Dispositivi.",
                );
                ui.add_space(8.0);
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
            }
        }
    }

    fn user_authenticate(&mut self, ctx: &egui::Context, register: bool) {
        let base = self.remote_url.trim().to_string();
        if !auth_origin_ok(&base) {
            self.pair_error =
                Some("Serve un indirizzo HTTPS valido (HTTP solo su questo computer).".into());
            return;
        }
        let client = match ApiClient::unauthenticated(&base) {
            Ok(client) => client,
            Err(error) => {
                self.pair_error = Some(error);
                return;
            }
        };
        let mut body = json!({
            "username": self.users.username.trim(),
            "password": self.users.password,
        });
        if register {
            body["display_name"] = json!(if self.users.display.trim().is_empty() {
                self.users.username.trim().to_string()
            } else {
                self.users.display.trim().to_string()
            });
            body["invite"] = json!(self.users.invite.trim());
        }
        self.users.password.clear();
        self.users.busy = true;
        self.pair_error = None;
        let tx = self.tx.clone();
        let data_dir = data_dir();
        std::thread::spawn(move || {
            let path = if register {
                "/api/auth/register"
            } else {
                "/api/auth/login"
            };
            let result = client.post(path, Some(&body)).and_then(|response| {
                let token = response["token"]
                    .as_str()
                    .ok_or("Sessione assente nella risposta del Core")?;
                std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
                std::fs::write(data_dir.join("remote-core-url.txt"), &base)
                    .map_err(|e| e.to_string())?;
                private_write(&data_dir.join("remote-user-token"), token)?;
                // Un dispositivo associato e un account non convivono: vale l'ultimo accesso.
                let _ = std::fs::remove_file(data_dir.join("remote-device-token"));
                ApiClient::user(&base, token.to_string()).map(|client| Connection {
                    client,
                    core: None,
                    remote: true,
                    user: true,
                })
            });
            let _ = tx.send(Event::Connected(Box::new(result)));
        });
        ctx.request_repaint();
    }

    /// Esce: chiude la sessione nel Core, dimentica il token e torna alla schermata di accesso.
    pub(super) fn user_logout(&mut self, ctx: &egui::Context) {
        if let Some(client) = self.client.clone() {
            std::thread::spawn(move || {
                let _ = client.post("/api/auth/logout", Some(&json!({})));
            });
        }
        let _ = std::fs::remove_file(data_dir().join("remote-user-token"));
        self.client = None;
        self.users = UserState::default();
        self.users.tab = 1;
        self.chat.clear();
        self.data = None;
        self.accent = None;
        self.current = "chat".into();
        self.error = Some("remote-pairing".into());
        ctx.request_repaint();
    }

    /// Carica cronologia e ricordi dell'utente appena connesso.
    pub(super) fn load_user_area(&mut self, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.users.area_loaded = true;
        let tx = self.tx.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = client
                .get("/api/me/history")
                .and_then(|history| {
                    client
                        .get("/api/me/memory")
                        .map(|memories| (history, memories))
                })
                .and_then(|(history, memories)| {
                    client.get("/api/me").map(|me| (history, memories, me))
                });
            let _ = tx.send(Event::UserArea(result));
            repaint.request_repaint();
        });
    }

    // ------------------------------------------------------------------ Chat dell'utente

    pub(super) fn show_user_chat(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.users.area_loaded {
            self.load_user_area(ctx);
        }
        let bottom = 60.0;
        let height = (ui.available_height() - bottom).max(80.0);
        egui::ScrollArea::vertical()
            .id_salt("user-chat")
            .max_height(height)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.chat.is_empty() {
                    ui.add_space(12.0);
                    ui.label(
                        "Scrivi a JARVIS. Per insegnargli come parlarti prova: «chiamami Marco», «dammi del tu», \
                         «sii più diretto», «ricorda che amo il caffè».",
                    );
                }
                for message in &self.chat {
                    let (label, color, fill) = if message.role == "user" {
                        ("TU", egui::Color32::from_rgb(112, 220, 218), SELECTED)
                    } else {
                        ("JARVIS", egui::Color32::from_rgb(180, 203, 222), RAISED)
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .corner_radius(12)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(egui::RichText::new(label).small().strong().color(color));
                            crate::markdown::show(
                                ui,
                                ctx,
                                &message.text,
                                egui::Color32::from_rgb(214, 226, 236),
                            );
                        });
                    ui.add_space(10.0);
                }
                if self.users.sending {
                    ui.label(egui::RichText::new("JARVIS sta scrivendo…").weak());
                }
            });
        if let Some(error) = self.error.clone() {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        ui.separator();
        ui.horizontal(|ui| {
            let input = ui.add_sized(
                [(ui.available_width() - 90.0).max(0.0), 42.0],
                egui::TextEdit::singleline(&mut self.users.input)
                    .char_limit(4000)
                    .hint_text("Messaggio a JARVIS…"),
            );
            let send = ui.add_enabled(!self.users.sending, egui::Button::new("Invia"));
            if send.clicked()
                || (input.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !self.users.sending)
            {
                self.send_user_message(ctx);
            }
        });
        if self.users.sending {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
    }

    fn send_user_message(&mut self, ctx: &egui::Context) {
        let message = self.users.input.trim().to_string();
        let Some(client) = self.client.clone() else {
            return;
        };
        if message.is_empty() {
            return;
        }
        self.users.input.clear();
        self.users.sending = true;
        self.error = None;
        self.chat.push(ChatMessage {
            role: "user",
            text: message.clone(),
        });
        let tx = self.tx.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = client.post("/api/me/chat", Some(&json!({"message": message})));
            let _ = tx.send(Event::UserReply(result));
            repaint.request_repaint();
        });
    }

    pub(super) fn apply_user_reply(&mut self, result: Result<Value, String>) {
        self.users.sending = false;
        match result {
            Ok(doc) => {
                self.error = None;
                self.chat.push(ChatMessage {
                    role: "assistant",
                    text: doc["reply"].as_str().unwrap_or_default().to_owned(),
                });
                // Un comando personale può aver cambiato ricordi o personalità: si ricaricano.
                if doc["personal"] == true {
                    self.users.area_loaded = false;
                    self.users.synced = false;
                    self.data = None;
                }
            }
            Err(error) => {
                // Il messaggio non è stato elaborato: si toglie dalla conversazione e si mostra l'errore.
                if self.chat.last().is_some_and(|m| m.role == "user") {
                    if let Some(last) = self.chat.pop() {
                        self.users.input = last.text;
                    }
                }
                self.error = Some(error);
            }
        }
    }

    pub(super) fn apply_user_area(&mut self, result: Result<(Value, Value, Value), String>) {
        match result {
            Ok((history, memories, me)) => {
                if !self.users.sending {
                    self.chat = parse_history(&history);
                }
                self.users.memories = memories["items"].as_array().cloned().unwrap_or_default();
                // L'avatar prende il colore della personalità dell'utente.
                self.accent =
                    accent_for(me["personality"]["profile"]["tone"].as_str().unwrap_or(""));
            }
            Err(error) => self.error = Some(error),
        }
    }

    // ------------------------------------------------------------------ Profilo e personalità

    pub(super) fn show_user_profile(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Il mio profilo");
            if ui.button("Aggiorna").clicked() {
                self.data = None;
                self.users.synced = false;
                self.users.area_loaded = false;
                self.refresh(ctx);
            }
            if self.loading {
                ui.add(egui::Spinner::new());
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(NEGATIVE, error);
        }
        if !self.users.area_loaded {
            self.load_user_area(ctx);
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        let personality = &data["personality"];
        if !self.users.synced {
            let (persona, profile) = (&personality["persona"], &personality["profile"]);
            self.users.display_name = data["user"]["display_name"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            self.users.address = persona["address"].as_str().unwrap_or("").to_owned();
            self.users.formal = persona["formal"].as_bool().unwrap_or(false);
            self.users.style = persona["style"].as_str().unwrap_or("film").to_owned();
            for (slot, key) in [
                (&mut self.users.tone, "tone"),
                (&mut self.users.level, "level"),
                (&mut self.users.role, "role"),
                (&mut self.users.interests, "interests"),
                (&mut self.users.goals, "goals"),
            ] {
                *slot = profile[key].as_str().unwrap_or("").to_owned();
            }
            self.accent = accent_for(&self.users.tone);
            self.users.synced = true;
        }

        egui::Frame::new()
            .fill(SURFACE)
            .corner_radius(12)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(egui::RichText::new("Come ti parla JARVIS").strong());
                for line in personality["summary"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    ui.label(format!("• {line}"));
                }
            });
        ui.add_space(10.0);

        ui.label(egui::RichText::new("Personalità").strong());
        egui::Grid::new("personality-form")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Nome");
                ui.add(egui::TextEdit::singleline(&mut self.users.display_name).char_limit(40));
                ui.end_row();
                ui.label("Come ti chiamo");
                ui.add(
                    egui::TextEdit::singleline(&mut self.users.address)
                        .char_limit(24)
                        .hint_text("nessun titolo"),
                );
                ui.end_row();
                ui.label("Forma");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.users.formal, false, "tu");
                    ui.selectable_value(&mut self.users.formal, true, "lei");
                });
                ui.end_row();
                ui.label("Stile");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.users.style, "film".to_owned(), "maggiordomo");
                    ui.selectable_value(&mut self.users.style, "sober".to_owned(), "sobrio");
                });
                ui.end_row();
                ui.label("Tono");
                ui.horizontal(|ui| {
                    for (value, label) in [
                        ("auto", "automatico"),
                        ("formale", "formale"),
                        ("cordiale", "cordiale"),
                        ("diretto", "diretto"),
                    ] {
                        ui.selectable_value(&mut self.users.tone, value.to_owned(), label);
                    }
                });
                ui.end_row();
                ui.label("Livello");
                ui.horizontal(|ui| {
                    for (value, label) in [
                        ("auto", "automatico"),
                        ("principiante", "principiante"),
                        ("intermedio", "intermedio"),
                        ("esperto", "esperto"),
                    ] {
                        ui.selectable_value(&mut self.users.level, value.to_owned(), label);
                    }
                });
                ui.end_row();
                ui.label("Ruolo");
                ui.add(egui::TextEdit::singleline(&mut self.users.role).char_limit(80));
                ui.end_row();
                ui.label("Interessi");
                ui.add(egui::TextEdit::singleline(&mut self.users.interests).char_limit(200));
                ui.end_row();
                ui.label("Obiettivi");
                ui.add(egui::TextEdit::singleline(&mut self.users.goals).char_limit(200));
                ui.end_row();
            });
        if ui.button("Salva la personalità").clicked() {
            let body = json!({
                "display_name": self.users.display_name.trim(),
                "persona": {"address": self.users.address.trim(), "formal": self.users.formal, "style": self.users.style},
                "profile": {"tone": self.users.tone, "level": self.users.level, "role": self.users.role.trim(),
                            "interests": self.users.interests.trim(), "goals": self.users.goals.trim()},
            });
            self.users.synced = false;
            self.mutate(ctx, Mutation::Put, "/api/me/personality".into(), Some(body));
        }

        ui.add_space(14.0);
        ui.label(egui::RichText::new("Ricordi").strong());
        if self.users.memories.is_empty() {
            ui.label(
                egui::RichText::new("Nessun ricordo: scrivi «ricorda che…» nella chat.").weak(),
            );
        }
        for memory in self.users.memories.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(memory["text"].as_str().unwrap_or(""));
                if ui.small_button("Dimentica").clicked() {
                    if let Some(id) = memory["id"].as_i64() {
                        self.users.area_loaded = false;
                        self.mutate(ctx, Mutation::Delete, format!("/api/me/memory/{id}"), None);
                    }
                }
            });
        }

        ui.add_space(14.0);
        ui.label(egui::RichText::new("Account").strong());
        ui.horizontal_wrapped(|ui| {
            if ui.button("Cancella la cronologia").clicked() {
                self.chat.clear();
                self.mutate(ctx, Mutation::Delete, "/api/me/history".into(), None);
            }
            if ui.button("Esci").clicked() {
                self.user_logout(ctx);
            }
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.users.pw_current)
                    .password(true)
                    .hint_text("Password attuale"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.users.pw_new)
                    .password(true)
                    .hint_text("Nuova password"),
            );
            if ui
                .add_enabled(
                    !self.users.pw_current.is_empty() && !self.users.pw_new.is_empty(),
                    egui::Button::new("Cambia password"),
                )
                .clicked()
            {
                let body = json!({"current": self.users.pw_current, "new": self.users.pw_new});
                self.users.pw_current.clear();
                self.users.pw_new.clear();
                self.password_change(ctx, body);
            }
        });
        ui.add_space(6.0);
        if self.users.delete_confirm {
            ui.colored_label(
                NEGATIVE,
                "Eliminare l'account con tutta la cronologia e i ricordi? Non si può annullare.",
            );
            ui.horizontal(|ui| {
                if ui.button("Sì, elimina tutto").clicked() {
                    self.delete_own_account(ctx);
                }
                if ui.button("Annulla").clicked() {
                    self.users.delete_confirm = false;
                }
            });
        } else if ui.small_button("Elimina il mio account…").clicked() {
            self.users.delete_confirm = true;
        }
    }

    /// Il cambio password restituisce una nuova sessione: il vecchio token non vale più.
    fn password_change(&mut self, ctx: &egui::Context, body: Value) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let tx = self.tx.clone();
        let base = self.remote_url.trim().to_string();
        let data_dir = data_dir();
        self.loading = true;
        std::thread::spawn(move || {
            let result = client
                .post("/api/me/password", Some(&body))
                .and_then(|doc| {
                    let token = doc["token"]
                        .as_str()
                        .ok_or("Sessione assente nella risposta")?;
                    private_write(&data_dir.join("remote-user-token"), token)?;
                    ApiClient::user(&base, token.to_string()).map(|client| Connection {
                        client,
                        core: None,
                        remote: true,
                        user: true,
                    })
                });
            let _ = tx.send(Event::Connected(Box::new(result)));
        });
        ctx.request_repaint();
    }

    fn delete_own_account(&mut self, ctx: &egui::Context) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let _ = client.delete("/api/me");
        self.user_logout(ctx);
    }

    // ------------------------------------------------------------------ Pagina del proprietario

    pub(super) fn show_users(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Utenti");
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
            "Le persone che inviti hanno un account con la loro conversazione, i loro ricordi e la loro personalità. \
             Non hanno strumenti e non vedono i tuoi dati; il testo dei loro messaggi non è visibile da qui.",
        );
        let Some(data) = self.data.clone() else {
            return;
        };
        let settings = &data["settings"];
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Chi può registrarsi").strong());
        ui.horizontal_wrapped(|ui| {
            for (value, label) in [
                ("closed", "Nessuno"),
                ("invite", "Solo su invito"),
                ("open", "Chiunque"),
            ] {
                if ui
                    .selectable_label(settings["registration"] == value, label)
                    .clicked()
                {
                    self.act(
                        ctx,
                        "/api/users/settings".into(),
                        Some(json!({"registration": value})),
                    );
                }
            }
        });
        let cloud = settings["cloud"].as_bool().unwrap_or(false);
        let mut shown = cloud;
        if ui
            .checkbox(
                &mut shown,
                "Gli utenti possono usare il mio modello cloud (costo tuo; i loro messaggi vanno al fornitore)",
            )
            .clicked()
        {
            self.act(
                ctx,
                "/api/users/settings".into(),
                Some(json!({"cloud": !cloud})),
            );
        }
        ui.add_space(8.0);
        if ui.button("Crea un invito").clicked() {
            self.act(ctx, "/api/users/invites".into(), Some(json!({})));
        }
        if let Some((code, expires)) = self.users.invite_shown.clone() {
            egui::Frame::new()
                .fill(SELECTED)
                .corner_radius(10)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Codice di invito (si vede solo ora)").strong());
                    ui.add(
                        egui::Label::new(egui::RichText::new(&code).monospace().size(16.0))
                            .selectable(true),
                    );
                    ui.small(format!("Vale una volta e scade: {expires}"));
                    if ui.small_button("Copia").clicked() {
                        ctx.copy_text(code.clone());
                    }
                });
        }
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(format!(
                "Account ({} su {})",
                data["items"].as_array().map_or(0, Vec::len),
                data["max"].as_i64().unwrap_or(50)
            ))
            .strong(),
        );
        for user in data["items"].as_array().into_iter().flatten() {
            let id = user["id"].as_str().unwrap_or("").to_owned();
            let disabled = user["disabled"].as_bool().unwrap_or(false);
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(10)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(user["display_name"].as_str().unwrap_or(""))
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "@{}",
                                user["username"].as_str().unwrap_or("")
                            ))
                            .weak(),
                        );
                        if disabled {
                            ui.colored_label(CAUTION, "disattivato");
                        }
                    });
                    ui.small(format!(
                        "{} messaggi · {} sessioni attive · ultimo accesso {}",
                        user["messages"],
                        user["sessions"],
                        user["last_login_at"]
                            .as_str()
                            .map_or("mai", |t| t.get(..16).unwrap_or(t))
                    ));
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .small_button(if disabled { "Riattiva" } else { "Disattiva" })
                            .clicked()
                        {
                            let action = if disabled { "enable" } else { "disable" };
                            self.act(ctx, format!("/api/users/{id}/{action}"), Some(json!({})));
                        }
                        if self.users.delete_pending.as_deref() == Some(id.as_str()) {
                            ui.colored_label(NEGATIVE, "Elimina l'account e tutti i suoi dati?");
                            if ui.small_button("Sì, elimina").clicked() {
                                self.users.delete_pending = None;
                                self.act(ctx, format!("/api/users/{id}/delete"), Some(json!({})));
                            }
                            if ui.small_button("Annulla").clicked() {
                                self.users.delete_pending = None;
                            }
                        } else if ui.small_button("Elimina…").clicked() {
                            self.users.delete_pending = Some(id.clone());
                        }
                    });
                });
            ui.add_space(6.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_origins_need_https_or_this_computer() {
        for ok in [
            "https://jarvis.example.com",
            "http://127.0.0.1:8765",
            "http://localhost:8765",
        ] {
            assert!(auth_origin_ok(ok), "{ok}");
        }
        for bad in [
            "http://jarvis.example.com",
            "ftp://x",
            "non un indirizzo",
            "https://",
            "http://192.168.1.5:8765",
        ] {
            assert!(!auth_origin_ok(bad), "{bad}");
        }
    }

    #[test]
    fn the_avatar_accent_follows_the_chosen_tone() {
        assert_eq!(accent_for("auto"), None);
        assert!(accent_for("formale").is_some() && accent_for("cordiale").is_some());
        assert_ne!(accent_for("formale"), accent_for("diretto"));
    }

    #[test]
    fn user_pages_are_owner_only_and_in_the_right_groups() {
        for (page, api, group) in [
            ("profile", "/api/me", "Assistente"),
            ("users", "/api/users", "Protezione"),
        ] {
            assert_eq!(endpoint(page), api);
            assert_eq!(nav_group(page), group);
            assert!(NAV
                .iter()
                .any(|(id, _, owner_only)| *id == page && *owner_only));
        }
    }
}
