//! Valutazione delle iniziative proattive (ADR-0097): ogni regola è deterministica, locale e scatta una sola
//! volta per chiave (giorno, settimana, oggetto) grazie a `proactive_sent`; durante le ore di silenzio non
//! scatta nulla e la regola riprova al giro successivo.

use super::*;
use crate::proactive::{self, Facts};
use chrono::{Local, Timelike};

/// Intervallo minimo fra due valutazioni nel pianificatore.
const CHECK_EVERY: Duration = Duration::from_secs(60);

impl Jarvis {
    /// Chiamata dal pianificatore: valuta le iniziative al massimo una volta al minuto.
    pub(super) fn proactive_poll(&mut self, now_utc: chrono::DateTime<chrono::Utc>) {
        if self
            .proactive_checked
            .is_some_and(|t| t.elapsed() < CHECK_EVERY)
        {
            return;
        }
        self.proactive_checked = Some(std::time::Instant::now());
        if let Err(e) = self.proactive_tick(now_utc.with_timezone(&Local)) {
            eprintln!("[proattività] errore: {e:?}");
        }
    }

    fn proactive_once(&self, key: &str, now: &chrono::DateTime<Local>) -> bool {
        let at = automation::ts(now.with_timezone(&chrono::Utc));
        self.store.proactive_claim(key, &at).unwrap_or(false)
    }

    /// Valuta tutte le regole all'istante locale `now`. Restituisce i tipi di iniziativa scattati.
    pub fn proactive_tick(
        &mut self,
        now: chrono::DateTime<Local>,
    ) -> Result<Vec<&'static str>, CoreError> {
        let s = proactive::settings(&self.cfg.data_dir);
        let mut fired = Vec::new();
        if s["enabled"] != true {
            return Ok(fired);
        }
        let time = |k: &str, d: &str| {
            proactive::parse_time(s[k].as_str().unwrap_or(d))
                .or_else(|| proactive::parse_time(d))
                .expect("orario predefinito valido")
        };
        let t = now.time();
        if proactive::in_quiet(t, time("quiet_from", "22:00"), time("quiet_to", "07:00")) {
            return Ok(fired);
        }
        let date = now.date_naive();
        let utc = now.with_timezone(&chrono::Utc);

        if s["briefing"] == true
            && proactive::briefing_window(t, time("briefing_time", "08:00"))
            && self.proactive_once(&format!("briefing:{date}"), &now)
        {
            self.send_briefing(now, &format!("briefing:{date}"))?;
            fired.push("briefing");
        }

        if s["disk"] == true {
            if let Ok(disks) = crate::toolbox::disks() {
                for (mount, free, used) in proactive::disk_alerts(&disks) {
                    if self.proactive_once(&format!("disk:{mount}:{date}"), &now) {
                        self.notify(json!({
                            "severity": "warning", "category": "system",
                            "title": format!("Spazio quasi esaurito su {mount}"),
                            "body": format!("{free} GB liberi, {used}% usato. Posso aiutarti a trovare i file più grandi."),
                            "source": {"kind": "module", "id": "proactive"},
                        }))?;
                        fired.push("disk");
                    }
                }
            }
        }

        if s["sandbox"] == true && crate::sandbox::enabled(&self.cfg.data_dir) {
            let sizes = crate::sandbox::session_sizes(&self.cfg.data_dir);
            for (session, mib) in proactive::sandbox_alerts(&sizes) {
                if self.proactive_once(&format!("sandbox:{session}:{date}"), &now) {
                    self.notify(json!({
                        "severity": "info", "category": "suggestion",
                        "title": format!("La sessione sandbox «{session}» è quasi piena"),
                        "body": format!("Usa {mib} MiB su 256: con il limite i nuovi comandi vengono rifiutati. Posso esportare i file utili e azzerarla."),
                        "source": {"kind": "module", "id": "proactive"}, "link": {"view": "chat"},
                    }))?;
                    fired.push("sandbox");
                }
            }
        }

        if s["confirmations"] == true {
            let limit = chrono::Duration::minutes(proactive::CONFIRMATION_NUDGE_MINUTES);
            let waiting: Vec<(String, String)> = self
                .pending
                .iter()
                .filter(|(_, p)| {
                    automation::parse_ts(p.task.doc["created_at"].as_str().unwrap_or_default())
                        .is_some_and(|c| utc - c >= limit)
                })
                .map(|(id, p)| {
                    (
                        id.clone(),
                        clip(p.task.doc["objective"].as_str().unwrap_or_default(), 100),
                    )
                })
                .collect();
            for (id, objective) in waiting {
                if self.proactive_once(&format!("confirm:{id}"), &now) {
                    self.notify(json!({
                        "severity": "info", "category": "confirmation",
                        "title": "Un'azione attende la tua conferma",
                        "body": format!("«{objective}» è ferma da più di {} minuti.", proactive::CONFIRMATION_NUDGE_MINUTES),
                        "source": {"kind": "module", "id": "proactive"}, "link": {"view": "chat"},
                    }))?;
                    fired.push("confirmations");
                }
            }
        }

        if s["workflows"] == true {
            for name in self.failing_workflows() {
                let key = format!("workflow:{}:{date}", name.0);
                if self.proactive_once(&key, &now) {
                    self.notify(json!({
                        "severity": "warning", "category": "suggestion",
                        "title": format!("L'automazione «{}» continua a fallire", name.1),
                        "body": format!("Le ultime {} esecuzioni sono fallite. Vuoi che controlli cosa non va?", proactive::WORKFLOW_FAILURES),
                        "source": {"kind": "module", "id": "proactive"}, "link": {"view": "automations"},
                    }))?;
                    fired.push("workflows");
                }
            }
        }

        if s["stale_todos"] == true {
            let cutoff = automation::ts(utc - chrono::Duration::days(proactive::STALE_DAYS));
            let stale: i64 = self
                .store
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM todos WHERE done_at IS NULL AND created_at < ?1",
                    [&cutoff],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if stale as u64 >= proactive::STALE_MIN
                && self.proactive_once(&format!("stale:{}", proactive::week_key(date)), &now)
            {
                self.notify(json!({
                    "severity": "info", "category": "suggestion",
                    "title": "Cose da fare lasciate indietro",
                    "body": format!("{stale} attività sono aperte da più di {} giorni. Vuoi rivederle insieme?", proactive::STALE_DAYS),
                    "source": {"kind": "module", "id": "proactive"}, "link": {"view": "chat"},
                }))?;
                fired.push("stale_todos");
            }
        }

        if now.hour() == 3 {
            let before = automation::ts(utc - chrono::Duration::days(30));
            let _ = self.store.proactive_prune(&before);
            // Conservazione (ADR-0124): tracce di esecuzione oltre 60 giorni e notifiche lette oltre 120.
            let _ = self
                .store
                .spans_prune(&automation::ts(utc - chrono::Duration::days(60)));
            let _ = self
                .store
                .notifications_prune(&automation::ts(utc - chrono::Duration::days(120)));
        }
        Ok(fired)
    }

    /// Automazioni attive le cui ultime esecuzioni sono tutte fallite: (id, nome).
    fn failing_workflows(&self) -> Vec<(String, String)> {
        let Ok(list) = self.store.workflows() else {
            return Vec::new();
        };
        list.into_iter()
            .filter(|(doc, _)| doc["enabled"] == true)
            .filter_map(|(doc, _)| {
                let id = doc["workflow_id"].as_str()?.to_string();
                let runs = self
                    .store
                    .runs(&id, proactive::WORKFLOW_FAILURES as u32)
                    .ok()?;
                (runs.len() >= proactive::WORKFLOW_FAILURES
                    && runs.iter().all(|r| r["status"] == "failed"))
                .then(|| (id, doc["name"].as_str().unwrap_or("senza nome").to_string()))
            })
            .collect()
    }

    fn briefing_facts(&self, now: chrono::DateTime<Local>) -> Facts {
        let mut f = Facts::from_date(now.date_naive(), now.hour());
        let utc = now.with_timezone(&chrono::Utc);
        let start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|d| d.and_local_timezone(Local).earliest())
            .map_or(utc, |d| d.with_timezone(&chrono::Utc));
        let end = automation::ts(start + chrono::Duration::days(1));
        let start = automation::ts(start);
        let count = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> u64 {
            self.store
                .conn
                .query_row(sql, params, |r| r.get::<_, i64>(0))
                .map_or(0, |n| n.max(0) as u64)
        };
        f.open_todos = count("SELECT COUNT(*) FROM todos WHERE done_at IS NULL", &[]);
        f.due_today = count(
            "SELECT COUNT(*) FROM todos WHERE done_at IS NULL AND due_at >= ?1 AND due_at < ?2",
            &[&start, &end],
        );
        f.overdue = count(
            "SELECT COUNT(*) FROM todos WHERE done_at IS NULL AND due_at < ?1",
            &[&start],
        );
        f.top = self
            .store
            .conn
            .query_row(
                "SELECT text FROM todos WHERE done_at IS NULL AND due_at IS NOT NULL AND due_at < ?1
                 ORDER BY (priority = 'alta') DESC, due_at LIMIT 1",
                [&end],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .map(|t| clip(&t, 120));
        f.confirmations = self.pending.len();
        f.unread = self
            .store
            .notifications_unread()
            .map(u64::from)
            .unwrap_or(0);
        f.missions = self
            .store
            .missions(50)
            .map(|m| m.iter().filter(|d| d["status"] == "active").count() as u64)
            .unwrap_or(0);
        f.failing = self.failing_workflows().into_iter().map(|w| w.1).collect();
        f
    }

    fn send_briefing(
        &mut self,
        now: chrono::DateTime<Local>,
        key: &str,
    ) -> Result<Value, CoreError> {
        let (title, body) = proactive::compose_briefing(&self.briefing_facts(now));
        let doc = self.notify(json!({
            "severity": "info", "category": "briefing", "title": title, "body": body,
            "source": {"kind": "module", "id": "proactive"}, "dedupe_key": key,
            "link": {"view": "chat"},
        }))?;
        Ok(json!({"title": title, "body": body, "notification": doc}))
    }

    /// Briefing su richiesta del proprietario, anche fuori orario e più volte al giorno.
    pub fn briefing_now(&mut self) -> Result<Value, CoreError> {
        let key = format!("briefing:now:{}", uuid::Uuid::new_v4().simple());
        self.send_briefing(chrono::Local::now(), &key)
    }

    pub fn proactive_settings(&self) -> Value {
        proactive::settings(&self.cfg.data_dir)
    }

    pub fn sandbox_settings(&self) -> Value {
        crate::sandbox::settings(&self.cfg.data_dir)
    }

    pub fn sandbox_update(&mut self, patch: &Value) -> Result<Value, CoreError> {
        let s = crate::sandbox::update(&self.cfg.data_dir, patch).map_err(CoreError::BadRequest)?;
        self.record_owner_action("sandbox.update")?;
        Ok(s)
    }

    pub fn proactive_update(&mut self, patch: &Value) -> Result<Value, CoreError> {
        let s = proactive::update(&self.cfg.data_dir, patch).map_err(CoreError::BadRequest)?;
        self.record_owner_action("proactive.update")?;
        Ok(s)
    }
}
