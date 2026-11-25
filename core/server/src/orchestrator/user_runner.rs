//! Utenti e personalità (ADR-0169): registrazione su invito, accesso, conversazione isolata e gestione da parte del
//! proprietario. Un utente non ha strumenti, non vede file, ricordi o conversazioni di altri e non tocca i segreti
//! del Core: la sua chat usa solo il modello cloud che il proprietario ha deciso di mettergli a disposizione.

use super::*;
use crate::accounts::{self, Command, UserRow};

/// Messaggi dell'utente consentiti in una finestra di 10 minuti.
const CHAT_LIMIT: i64 = 30;
const HISTORY_FOR_MODEL: i64 = 12;

/// Esito del primo tempo di un messaggio: risposta già pronta o richiesta da inviare al modello (fuori dal blocco).
pub enum UserTurn {
    Done(Value),
    Ask(Box<crate::cloud::AgentRequest>, crate::cloud::Provider),
}

impl Jarvis {
    fn users_dir(&self, id: &str) -> std::path::PathBuf {
        self.cfg.data_dir.join("users").join(id)
    }

    fn user_audit(&mut self, actor: &str, action: &str, resource: &str, outcome: &str) {
        let _ = self.audit(json!({
            "correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "client", "id": actor},
            "node": "core-local", "action": action, "resource": format!("jarvis://user/{resource}"),
            "outcome": outcome, "effect": "applied"}));
    }

    // -------------------------------------------------------------- Proprietario

    /// Utenti, regole di accesso e dati d'uso: niente impronte, niente contenuti.
    pub fn users_overview(&self) -> Result<Value, CoreError> {
        Ok(json!({
            "items": accounts::list_users(&self.store).map_err(internal)?,
            "settings": accounts::settings(&self.cfg.data_dir),
            "max": accounts::MAX_USERS,
            "policy": "Gli utenti non hanno strumenti e non vedono i tuoi dati: chattano con il modello cloud solo se lo attivi qui. Il testo dei loro messaggi non è visibile da questa pagina.",
        }))
    }

    pub fn users_settings_set(&mut self, patch: &Value) -> Result<Value, CoreError> {
        let out =
            accounts::set_settings(&self.cfg.data_dir, patch).map_err(CoreError::BadRequest)?;
        self.record_owner_action("users.settings")?;
        Ok(out)
    }

    /// Crea un invito monouso; il codice si vede solo adesso.
    pub fn user_invite_create(&mut self) -> Result<Value, CoreError> {
        let (code, expires) = accounts::create_invite(&self.store).map_err(internal)?;
        self.record_owner_action("users.invite")?;
        Ok(json!({"code": code, "expires_at": expires,
            "note": "Mostralo solo alla persona invitata: vale una volta e scade fra 72 ore."}))
    }

    pub fn user_set_disabled(&mut self, id: &str, disabled: bool) -> Result<Value, CoreError> {
        if !accounts::set_disabled(&self.store, id, disabled).map_err(internal)? {
            return Err(CoreError::NotFound("utente non trovato".into()));
        }
        self.record_owner_action(if disabled {
            "users.disable"
        } else {
            "users.enable"
        })?;
        self.users_overview()
    }

    pub fn user_delete(&mut self, id: &str) -> Result<Value, CoreError> {
        if !accounts::delete_user(&self.store, id).map_err(internal)? {
            return Err(CoreError::NotFound("utente non trovato".into()));
        }
        let _ = std::fs::remove_dir_all(self.users_dir(id));
        self.record_owner_action("users.delete")?;
        self.users_overview()
    }

    // -------------------------------------------------------------- Accesso

    /// Controlli economici di una registrazione, prima di spendere un calcolo di Argon2 per la password.
    pub fn user_register_precheck(&self, body: &Value) -> Result<(), CoreError> {
        let settings = accounts::settings(&self.cfg.data_dir);
        let invite = body["invite"].as_str().unwrap_or_default().trim();
        match settings["registration"].as_str().unwrap_or("invite") {
            "closed" => {
                return Err(CoreError::Unauthorized(
                    "Le registrazioni sono chiuse: chiedi un invito all'amministratore.".into(),
                ))
            }
            "invite" if !accounts::invite_valid(&self.store, invite).map_err(internal)? => {
                return Err(CoreError::Unauthorized(
                    "Invito non valido o scaduto.".into(),
                ))
            }
            _ => {}
        }
        let username = body["username"].as_str().unwrap_or_default();
        if accounts::user_by_name(&self.store, username)
            .map_err(internal)?
            .is_some()
        {
            return Err(CoreError::BadRequest("Nome utente già in uso.".into()));
        }
        if accounts::user_count(&self.store).map_err(internal)? >= accounts::MAX_USERS {
            return Err(CoreError::Unavailable(
                "Numero massimo di utenti raggiunto.".into(),
            ));
        }
        Ok(())
    }

    /// Registra un utente. `pw_hash` è già calcolato (fuori dal blocco del Core) e la password validata.
    pub fn user_register(&mut self, body: &Value, pw_hash: &str) -> Result<Value, CoreError> {
        let settings = accounts::settings(&self.cfg.data_dir);
        let mode = settings["registration"].as_str().unwrap_or("invite");
        let invite = body["invite"].as_str().unwrap_or_default().trim();
        match mode {
            "closed" => {
                return Err(CoreError::Unauthorized(
                    "Le registrazioni sono chiuse: chiedi un invito all'amministratore.".into(),
                ))
            }
            "invite" if !accounts::invite_valid(&self.store, invite).map_err(internal)? => {
                return Err(CoreError::Unauthorized(
                    "Invito non valido o scaduto.".into(),
                ))
            }
            _ => {}
        }
        let username = body["username"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        if !accounts::valid_username(&username) {
            return Err(CoreError::BadRequest(
                "Nome utente: da 3 a 32 caratteri a-z, 0-9, punto, trattino e trattino basso"
                    .into(),
            ));
        }
        let display = body["display_name"]
            .as_str()
            .unwrap_or(&username)
            .trim()
            .to_string();
        if !accounts::valid_display_name(&display) {
            return Err(CoreError::BadRequest(
                "Il nome mostrato ha al massimo 40 lettere, numeri, spazi, apostrofi o trattini"
                    .into(),
            ));
        }
        if accounts::user_count(&self.store).map_err(internal)? >= accounts::MAX_USERS {
            return Err(CoreError::Unavailable(
                "Numero massimo di utenti raggiunto.".into(),
            ));
        }
        let Some(id) =
            accounts::create_user(&self.store, &username, &display, pw_hash).map_err(internal)?
        else {
            return Err(CoreError::BadRequest("Nome utente già in uso.".into()));
        };
        let created = (|| -> Result<(), CoreError> {
            if mode == "invite"
                && !accounts::consume_invite(&self.store, invite, &id).map_err(internal)?
            {
                return Err(CoreError::Unauthorized(
                    "Invito non valido o scaduto.".into(),
                ));
            }
            let dir = self.users_dir(&id);
            let parent = self.cfg.data_dir.join("users");
            if let Err(e) = crate::backup::private_dir(&parent) {
                if !(e.kind() == std::io::ErrorKind::AlreadyExists && parent.is_dir()) {
                    return Err(CoreError::Internal(e.to_string()));
                }
            }
            crate::backup::private_dir(&dir).map_err(|e| CoreError::Internal(e.to_string()))?;
            // Personalità iniziale: si rivolge all'utente per nome, in modo informale; le risposte
            // al questionario di registrazione (facoltative) la rifiniscono.
            let mut persona = json!({"formal": false, "address_frequency": "rare"});
            if crate::persona::valid_address(&display) {
                persona["address"] = json!(display);
            }
            if let Some(extra) = body["personality"]["persona"].as_object() {
                for (k, v) in extra {
                    persona[k] = v.clone();
                }
            }
            crate::persona::update_for(&dir, &persona).map_err(CoreError::BadRequest)?;
            if let Some(profile) = body["personality"]["profile"].as_object() {
                crate::profile::update(&dir, &Value::Object(profile.clone()))
                    .map_err(CoreError::BadRequest)?;
            }
            Ok(())
        })();
        if let Err(e) = created {
            let _ = accounts::delete_user(&self.store, &id);
            let _ = std::fs::remove_dir_all(self.users_dir(&id));
            return Err(e);
        }
        let token = accounts::new_session(&self.store, &id).map_err(internal)?;
        self.user_audit(&id, "user.register", &id, "succeeded");
        let user = accounts::user_by_id(&self.store, &id)
            .map_err(internal)?
            .ok_or_else(|| CoreError::Internal("utente non trovato dopo la creazione".into()))?;
        Ok(json!({"token": token, "user": user.view(), "personality": self.user_personality(&id)}))
    }

    /// Primo tempo dell'accesso: l'utente da verificare (la password si controlla fuori dal blocco).
    pub fn user_login_lookup(&self, username: &str) -> Result<Option<UserRow>, CoreError> {
        accounts::user_by_name(&self.store, &username.trim().to_lowercase()).map_err(internal)
    }

    /// Secondo tempo, a password verificata: apre la sessione.
    pub fn user_login_finish(&mut self, id: &str) -> Result<Value, CoreError> {
        let user = accounts::user_by_id(&self.store, id)
            .map_err(internal)?
            .filter(|u| !u.disabled)
            .ok_or_else(|| CoreError::Unauthorized("Credenziali non valide.".into()))?;
        let token = accounts::new_session(&self.store, id).map_err(internal)?;
        self.user_audit(id, "user.login", id, "succeeded");
        Ok(json!({"token": token, "user": user.view(), "personality": self.user_personality(id)}))
    }

    pub fn user_login_failed(&mut self, username: &str) {
        let name: String = username.chars().take(32).collect();
        let _ = self.audit(json!({
            "correlation_id": uuid::Uuid::new_v4().to_string(), "actor": {"kind": "client", "id": "anonymous"},
            "node": "core-local", "action": "user.login", "resource": format!("jarvis://user/{name}"),
            "outcome": "failed", "effect": "not_applied", "error_code": "auth.invalid_credentials"}));
    }

    pub fn user_for_session(&self, token: &str) -> Option<UserRow> {
        accounts::session_user(&self.store, token).ok().flatten()
    }

    pub fn user_logout(&mut self, token: &str, id: &str) -> Result<Value, CoreError> {
        accounts::end_session(&self.store, token).map_err(internal)?;
        self.user_audit(id, "user.logout", id, "succeeded");
        Ok(json!({"logged_out": true}))
    }

    // -------------------------------------------------------------- Personalità

    /// Personalità dell'utente: come JARVIS gli parla e che cosa sa di lui, con un riepilogo leggibile.
    pub fn user_personality(&self, id: &str) -> Value {
        let dir = self.users_dir(id);
        let persona = crate::persona::load(&dir);
        let profile = crate::profile::load(&dir);
        let name = accounts::user_by_id(&self.store, id)
            .ok()
            .flatten()
            .map(|u| u.display_name)
            .unwrap_or_default();
        json!({
            "persona": crate::persona::to_json(&persona),
            "profile": crate::profile::to_json(&profile),
            "summary": personality_summary(&name, &persona, &profile),
        })
    }

    pub fn user_me(&self, user: &UserRow) -> Value {
        json!({"user": user.view(), "personality": self.user_personality(&user.id),
            "cloud": accounts::settings(&self.cfg.data_dir)["cloud"]})
    }

    /// Aggiorna nome mostrato, personalità e profilo; valida tutto prima di scrivere.
    pub fn user_personality_update(
        &mut self,
        user: &UserRow,
        body: &Value,
    ) -> Result<Value, CoreError> {
        if let Some(name) = body["display_name"].as_str() {
            if !accounts::valid_display_name(name) {
                return Err(CoreError::BadRequest(
                    "Il nome mostrato ha al massimo 40 lettere, numeri, spazi, apostrofi o trattini".into(),
                ));
            }
        }
        let dir = self.users_dir(&user.id);
        if let Some(persona) = body.get("persona").filter(|p| p.is_object()) {
            crate::persona::update_for(&dir, persona).map_err(CoreError::BadRequest)?;
        }
        if let Some(profile) = body.get("profile").filter(|p| p.is_object()) {
            crate::profile::update(&dir, profile).map_err(CoreError::BadRequest)?;
        }
        if let Some(name) = body["display_name"].as_str() {
            accounts::set_display_name(&self.store, &user.id, name).map_err(internal)?;
        }
        self.user_audit(&user.id, "user.personality", &user.id, "succeeded");
        Ok(self.user_personality(&user.id))
    }

    /// Cambio password: la verifica della precedente è già avvenuta; chiude le altre sessioni.
    pub fn user_set_password(
        &mut self,
        user: &UserRow,
        new_hash: &str,
        keep_token: &str,
    ) -> Result<Value, CoreError> {
        accounts::set_password(&self.store, &user.id, new_hash).map_err(internal)?;
        accounts::end_sessions(&self.store, &user.id).map_err(internal)?;
        // La sessione corrente resta valida: se ne apre una nuova con lo stesso scopo.
        let _ = keep_token;
        let token = accounts::new_session(&self.store, &user.id).map_err(internal)?;
        self.user_audit(&user.id, "user.password", &user.id, "succeeded");
        Ok(json!({"token": token}))
    }

    // -------------------------------------------------------------- Cronologia e ricordi

    pub fn user_history(&self, user: &UserRow) -> Result<Value, CoreError> {
        Ok(json!({"items": accounts::history(&self.store, &user.id, 100).map_err(internal)?}))
    }

    pub fn user_history_clear(&mut self, user: &UserRow) -> Result<Value, CoreError> {
        accounts::clear_history(&self.store, &user.id).map_err(internal)?;
        Ok(json!({"cleared": true}))
    }

    pub fn user_memories(&self, user: &UserRow) -> Result<Value, CoreError> {
        let items: Vec<Value> = accounts::memories(&self.store, &user.id)
            .map_err(internal)?
            .into_iter()
            .map(|(id, text)| json!({"id": id, "text": text}))
            .collect();
        Ok(json!({"items": items, "max": accounts::MAX_MEMORIES_PER_USER}))
    }

    pub fn user_memory_delete(&mut self, user: &UserRow, id: i64) -> Result<Value, CoreError> {
        if !accounts::delete_memory(&self.store, &user.id, id).map_err(internal)? {
            return Err(CoreError::NotFound("ricordo non trovato".into()));
        }
        self.user_memories(user)
    }

    /// L'utente elimina il proprio account e tutti i suoi dati.
    pub fn user_delete_self(&mut self, user: &UserRow) -> Result<Value, CoreError> {
        self.user_audit(&user.id, "user.delete", &user.id, "succeeded");
        accounts::delete_user(&self.store, &user.id).map_err(internal)?;
        let _ = std::fs::remove_dir_all(self.users_dir(&user.id));
        Ok(json!({"deleted": true}))
    }

    // -------------------------------------------------------------- Chat

    /// Primo tempo di un messaggio dell'utente: comandi personali o richiesta per il modello.
    pub fn user_turn(&mut self, user: &UserRow, message: &str) -> Result<UserTurn, CoreError> {
        let message = message.trim();
        if message.is_empty() || message.chars().count() > 4000 {
            return Err(CoreError::BadRequest(
                "Il messaggio deve avere da 1 a 4000 caratteri".into(),
            ));
        }
        if crate::cloud::contains_credentials(message) {
            return Err(CoreError::BadRequest(
                "Il messaggio sembra contenere una credenziale: non lo invio.".into(),
            ));
        }
        let since = (chrono::Utc::now() - chrono::Duration::minutes(10))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        if accounts::recent_user_messages(&self.store, &user.id, &since).map_err(internal)?
            >= CHAT_LIMIT
        {
            return Err(CoreError::Unavailable(
                "Troppi messaggi in poco tempo: riprova fra qualche minuto.".into(),
            ));
        }
        let dir = self.users_dir(&user.id);
        // Comandi personali, eseguiti senza interrogare il modello.
        if let Some(command) = accounts::parse_command(message) {
            let reply = match command {
                Command::Remember(text) => {
                    if crate::cloud::contains_credentials(&text) {
                        return Err(CoreError::BadRequest("Non memorizzo credenziali.".into()));
                    }
                    match accounts::add_memory(&self.store, &user.id, &text).map_err(internal)? {
                        Some(_) => "Fatto: me lo ricorderò. Puoi vedere e cancellare i ricordi dal tuo profilo.".to_string(),
                        None => "Ho già troppi ricordi: cancellane qualcuno dal tuo profilo e riprova.".to_string(),
                    }
                }
                Command::ForgetAll => {
                    accounts::clear_memories(&self.store, &user.id).map_err(internal)?;
                    "Ho dimenticato tutti i tuoi ricordi.".to_string()
                }
                Command::CallMe(name) => {
                    crate::persona::update_for(
                        &dir,
                        &json!({"address": name, "address_frequency": "rare"}),
                    )
                    .map_err(CoreError::BadRequest)?;
                    format!("Va bene, ti chiamerò {name}.")
                }
                Command::Formal(formal) => {
                    crate::persona::update_for(&dir, &json!({"formal": formal}))
                        .map_err(CoreError::BadRequest)?;
                    if formal {
                        "Certamente: da ora ti darò del lei.".to_string()
                    } else {
                        "Volentieri: da ora ci diamo del tu.".to_string()
                    }
                }
                Command::Tone(tone) => {
                    crate::profile::update(&dir, &json!({"tone": tone}))
                        .map_err(CoreError::BadRequest)?;
                    format!("D'accordo: da ora il mio tono sarà {tone}.")
                }
            };
            accounts::add_message(&self.store, &user.id, "user", message).map_err(internal)?;
            accounts::add_message(&self.store, &user.id, "assistant", &reply).map_err(internal)?;
            return Ok(UserTurn::Done(json!({"reply": reply, "personal": true})));
        }
        if accounts::settings(&self.cfg.data_dir)["cloud"] != true {
            return Err(CoreError::Unavailable(
                "L'amministratore non ha ancora attivato il modello per gli utenti.".into(),
            ));
        }
        let Some((provider, model, key)) = self.mission_provider(message)? else {
            return Err(CoreError::Unavailable(
                "Nessun modello cloud è configurato: chiedi all'amministratore.".into(),
            ));
        };
        let persona = crate::persona::load(&dir);
        let profile = crate::profile::load(&dir);
        let memories: Vec<String> = accounts::memories(&self.store, &user.id)
            .map_err(internal)?
            .into_iter()
            .rev()
            .take(20)
            .map(|(_, text)| text)
            .collect();
        let system = crate::prompts::user_system(
            message,
            &crate::persona::guidance(&persona),
            &crate::profile::guidance(&profile),
            &memories,
        );
        let history: Vec<(String, String)> =
            accounts::history(&self.store, &user.id, HISTORY_FOR_MODEL)
                .map_err(internal)?
                .into_iter()
                .map(|m| {
                    (
                        m["role"].as_str().unwrap_or("user").to_string(),
                        m["content"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect();
        let request = crate::cloud::AgentRequest::new(
            provider,
            model,
            key,
            system,
            message.to_string(),
            Vec::new(),
            Vec::new(),
            self.cloud_limiter.clone(),
        )
        .with_history(history);
        Ok(UserTurn::Ask(Box::new(request), provider))
    }

    /// Secondo tempo: registra lo scambio e restituisce la risposta.
    pub fn user_turn_finish(
        &mut self,
        user: &UserRow,
        message: &str,
        answer: Result<crate::cloud::Answer, crate::cloud::Failure>,
        provider: crate::cloud::Provider,
    ) -> Result<Value, CoreError> {
        let reply = match answer {
            Ok(crate::cloud::Answer::Text(text)) if !text.trim().is_empty() => {
                text.trim().to_string()
            }
            Ok(_) => {
                return Err(CoreError::Unavailable(
                    "Il modello non ha dato una risposta: riprova.".into(),
                ))
            }
            Err(failure) => return Err(CoreError::Unavailable(failure.user_message(provider).0)),
        };
        accounts::add_message(&self.store, &user.id, "user", message.trim()).map_err(internal)?;
        accounts::add_message(&self.store, &user.id, "assistant", &reply).map_err(internal)?;
        Ok(json!({"reply": reply}))
    }
}

/// Riepilogo leggibile della personalità: come JARVIS parla a questa persona.
pub fn personality_summary(
    name: &str,
    persona: &crate::persona::Persona,
    profile: &crate::profile::Profile,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(match persona.style {
        crate::persona::Style::Film => {
            "Stile: maggiordomo digitale, pacato e con ironia asciutta".to_string()
        }
        crate::persona::Style::Sober => "Stile: sobrio ed essenziale".to_string(),
    });
    lines.push(
        if persona.address.is_empty() || persona.frequency == crate::persona::Frequency::Never {
            format!(
                "Ti chiamo senza titoli{}",
                if name.is_empty() {
                    String::new()
                } else {
                    format!(" ({name})")
                }
            )
        } else {
            format!("Ti chiamo «{}»", persona.address)
        },
    );
    lines.push(if persona.formal {
        "Ti do del «lei»".to_string()
    } else {
        "Ti do del «tu»".to_string()
    });
    for (label, key) in [
        ("Tono", "tone"),
        ("Livello", "level"),
        ("Lingua", "language"),
        ("Ruolo", "role"),
        ("Interessi", "interests"),
        ("Obiettivi", "goals"),
    ] {
        let value = profile.get(key);
        if !value.is_empty() && value != "auto" {
            lines.push(format!("{label}: {value}"));
        }
    }
    lines
}
