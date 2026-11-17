# Roadmap JARVIS

Il piano approvato segue sette fasi. Le condizioni di uscita qui sotto rendono verificabile l'avanzamento. Lo stato effettivo si ricava dal repository e dalle evidenze associate al commit.

| Fase | Scope | Condizione di uscita |
|---|---|---|
| 1. Architettura e contratti | Confini dei moduli, monorepo, identità, contratti e requisiti | Contratti iniziali verificabili e dipendenze documentate |
| 2. Prototipi tecnici | Runtime locale/cloud, integrazione Rust/Python, memoria e interfacce, operazioni rappresentative | Prove riproducibili e limiti hardware registrati |
| 3. MVP avanzato | Prima versione integrata delle capacità sotto elencate | Flussi principali dimostrati con test ed evidenze |
| 4. Integrazione | Robustezza dei sottosistemi, connettori e consolidamento delle verifiche | Interazioni fra moduli e gestione degli errori verificate |
| 5. Espansione | Mobile completo, avatar 3D, orchestrazione hardware distribuita e sincronizzazione evoluta | Evidenze di compatibilità, offline e ripresa per i nuovi nodi |
| 6. Validazione | Sicurezza, SLO, compatibilità, guasti, recovery, deploy e aggiornamenti | Evidenze raccolte secondo il sistema di accettazione |
| 7. Release e manutenzione | Accettazione, pubblicazione privata, canali e manutenzione | Accettazione del proprietario, release riproducibile e procedure operative |

La prima versione include Core, Orchestrator, agenti fondamentali, AI locale e cloud, memoria, Computer Use, voce, Web Agent, automazioni, sicurezza e Desktop/Web. L'Integration Engine fornisce il framework e il catalogo iniziale. Il dettaglio dei connettori e l'ordine di lavoro si definiscono tramite dipendenze e prove tecniche.

## Checkpoint del Project Continuity Engine

Il checkpoint è la sezione di ripresa versionata con Git. Ogni voce va aggiornata quando cambia lo stato verificato. Includere gli identificativi dei requisiti e percorsi alle prove.

| Campo | Stato al 2026-11-17 (ottava ripresa) |
|---|---|
| Ultima revisione verificata | Base `b52dbbc` su `main`: la PR #7 (modifica delle automazioni) è stata unita con CI verde. Il lavoro di questa ripresa è sul branch `claude/cool-ramanujan-bogarl` in una nuova PR, da unire automaticamente con CI verde |
| Milestone effettiva | **Fase 3 — MVP avanzato, in corso.** Completati RF-07, RF-13 e RF-06; RF-10 desktop disponibile come guscio Tauri (ADR-0009) |
| Risultato disponibile | Tutto quanto sopra, più l'app desktop:<br>• `./jarvis.sh desktop` apre una finestra Tauri che avvia il Core su una porta libera con i dati per utente e ne mostra l'interfaccia;<br>• navigazione limitata, link esterni nel browser di sistema, nessuna capability nativa;<br>• arresto del Core alla chiusura e alla morte del padre (`JARVIS_EXIT_WITH_PARENT=stdin`) |
| Verifiche del software | Container remoto Linux, Python 3.13.18, Rust 1.97.0, WebKitGTK 2.52 (installato in questa sessione), Tauri 2.12.6.<br>• Python: 9 + 23 test OK;<br>• `cargo test --locked` in `core/`: 66 test OK (62 più 4 del launcher con il Core reale);<br>• `cargo clippy` e `cargo fmt` OK sia in `core/` sia in `apps/desktop/src-tauri`;<br>• prova manuale con Xvfb: finestra con l'interfaccia del Core; dopo SIGKILL dell'app i processi Core e worker passano da 2 a 0;<br>• E2E: 30 verifiche OK nella ripresa precedente, interfaccia invariata.<br>La CI aggiunge il job `desktop` su Linux, macOS e Windows, con esito da leggere sulla PR |
| Controlli sospesi | Web reale e Ollama con un modello reale: non disponibili nel container. Avvio dell'app desktop su macOS e Windows: solo build in CI, nessuna esecuzione interattiva |
| Lacune documentali | Risposte 75–80 senza fonte, lasciate vuote. Restano aperte: RPC ed event bus, PostgreSQL, segreti, VPN mesh, motori vocali, stack mobile, SLO, firma dell'audit, canali di notifica esterni, navigazione con Playwright, installatori desktop firmati con aggiornamenti, React per l'interfaccia |
| Blocchi | Nessuno |
| Prossima azione | Fase 3, nell'ordine:<br>1. voce (RF-09): scelta di motori STT, TTS e wake word locali (da decidere con il proprietario), poi pipeline con interruzione;<br>2. installatore desktop autonomo (Core, runtime Python, firma, canali) per la Fase 6;<br>3. navigazione interattiva con Playwright (estensione di RF-06) |

### Registro delle riprese

| Data | Revisione di partenza | Lavoro | Verifiche | Prossimo passo |
|---|---|---|---|---|
| 2026-11-17 | `ad1972c` | Inventario: solo documentazione in `docs/`. Creati contratti v1, fixture sintetiche, test, CI, `CLAUDE.md` in radice, ADR-0001 | 4 test unittest OK | Contratti `task` e `policy-decision` |
| 2026-11-17 | `f5aba7b` | CI rieseguita e verde dopo che il repository è stato reso pubblico. Contratti task, policy-decision e audit-event; matrice delle dipendenze; chiusura tecnica della Fase 1. Fase 2: crate Rust `jarvis-contracts`, job CI Rust, ADR-0002, tracciabilità dei requisiti | Python 8 OK, Rust 5 OK, clippy e fmt OK | Bridge Rust–Python su stdio |
| 2026-11-17 | `d482b80` | Branch ricreato da `main` dopo il merge della PR #1. Corretti i collegamenti di `CLAUDE.md` e `README.md`, spostati in radice dal proprietario. Validazione Python spostata in `ai/jarvis_ai/contracts.py`. Bridge stdio (`core/bridge`, `ai/jarvis_ai/worker.py`), ADR-0003, CI estesa | Python 8+6 OK, Rust 11 OK, clippy e fmt OK | AI Gateway con adapter sintetico e adapter Ollama |
| 2026-11-17 | `795986d` | PR #2 unita con l'approvazione del proprietario; autorizzato il merge automatico con CI verde. Prototipo integrato v0.1: contratto `agent-result`, catalogo degli strumenti, AI Gateway (Ollama e offline), Core HTTP con policy, conferme, SQLite, audit e interfaccia Web, `jarvis.sh`, test API ed E2E, job CI E2E, ADR-0004 e specifiche aggiornate | Python 9+17 OK, Rust 23 OK, E2E 12/12 OK | Riscontro del proprietario con Ollama reale; Automation Engine (RF-07) |
| 2026-11-17 | `5330b3c` | PR #3 unita con CI verde. Fase 3, RF-07: contratti workflow e workflow-run, strumenti `workflow.*`, Automation Engine nel Core con pianificatore, approvazione legata al digest dei passi, policy in background, recupero dopo riavvio, cronologia; intenti offline di pianificazione; sezione Automazioni dell'interfaccia; ADR-0005; percorso Python relativo risolto all'avvio | Python 9+19 OK, Rust 38 OK, E2E 17/17 OK, pianificatore reale verificato | RF-13 notifiche, poi RF-06 Web |
| 2026-11-17 | `b6f5afa` | PR #4 unita con CI verde. RF-13: contratto `notification`, notifiche correlate e deduplicate, monitor nel pianificatore, API delle notifiche, campanella, toast e notifiche del browser; ADR-0006 | Python 9+19 OK, Rust 45 OK, E2E 21/21 OK | RF-06 Web Agent |
| 2026-11-17 | `cb528da` | PR #5 unita con CI verde. RF-06: `web.search` e `web.fetch`, policy Web (SSRF, domini, interruttore), fonti con SHA-256 salvate con i messaggi, difesa dalla prompt injection, intenti offline e prompt Ollama; conferma leggibile per `workflow.create`; ADR-0007 | Python 9+22 OK, Rust 56 OK, E2E 26/26 OK | Modifica delle automazioni, poi Tauri |
| 2026-11-17 | `8de6fca` | PR #6 unita con CI verde. Modifica delle automazioni con versioni e nuova approvazione (`workflow.update`, `PUT`, `/versions`, intenti offline, pulsante Modifica); corretti il contatore notifiche e i toast; ADR-0008 | Python 9+23 OK, Rust 62 OK, E2E 30/30 OK | Shell desktop Tauri |
| 2026-11-17 | `b52dbbc` | PR #7 unita con CI verde. App desktop Tauri (`apps/desktop`): launcher testato, finestra con navigazione limitata, dati per utente, arresto alla morte del padre (bug degli orfani trovato con SIGKILL e corretto); `./jarvis.sh desktop`; CI desktop su 3 sistemi; ADR-0009 | Rust 66 OK, desktop clippy e build OK, prova con Xvfb OK | Voce (RF-09) |

Per ogni ripresa registrare data, revisione, lavoro completato, verifiche eseguite, blocchi, ipotesi e prossimo passo concreto. Conservare il checkpoint con le modifiche alle quali si riferisce.
