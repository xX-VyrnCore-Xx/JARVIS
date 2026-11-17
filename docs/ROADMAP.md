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

| Campo | Stato al 2026-11-17 (decima ripresa) |
|---|---|
| Ultima revisione verificata | Base `9d4a505` su `main` più la PR #10 (avvio su 3 sistemi). Il lavoro di questa ripresa è sul branch `claude/cool-ramanujan-bogarl`, da unire automaticamente con CI verde |
| Milestone effettiva | **Fase 3 chiusa tecnicamente** (RF-06, RF-07, RF-09, RF-10 desktop, RF-13; manca l'accettazione del proprietario dopo i test sul suo computer, rimandati alla fine su sua richiesta). **Fase 4 — Integrazione, in corso**: osservabilità, iniezione di guasti, avvio multipiattaforma e primi SLO (ADR-0011) |
| Risultato disponibile | Tutto quanto sopra, più:<br>• lanciatori affidabili (`jarvis.sh` con `doctor`, `jarvis.cmd` e `jarvis.ps1`, apertura automatica del browser);<br>• `/api/metrics` e log `data/logs/core.jsonl` correlati e senza contenuti;<br>• pannello Sistema;<br>• guasti iniettabili nel worker con recupero verificato;<br>• misure di latenza confrontate con `tests/perf/slo.json` |
| Verifiche del software | Container remoto Linux.<br>• Python: 9 + 33 test OK;<br>• `cargo test --locked`: 85 test OK (9 di robustezza, 2 unitari di osservabilità);<br>• clippy e fmt OK;<br>• E2E: 40 verifiche OK;<br>• `tests/perf/latency.py`: p95 health 3,4 ms, chat 9,1 ms, chat con strumento 15,3 ms.<br>CI della PR #10: avvio reale verde su Linux e macOS; Windows in corso al momento della scrittura |
| Controlli sospesi | Test sul computer del proprietario (rimandati alla fine): Web reale, Ollama, voce con modelli reali, microfono nell'app desktop. SLO con modelli reali |
| Lacune documentali | Risposte 75–80 senza fonte, lasciate vuote. Restano aperte: RPC ed event bus, PostgreSQL, segreti, VPN mesh, stack mobile, firma dell'audit, canali di notifica esterni, navigazione con Playwright, installatori firmati, React, trace distribuite |
| Blocchi | Nessuno |
| Prossima azione | Fase 4, nell'ordine:<br>1. ~~RNF-08: backup cifrato e ripristino verificato~~ fatto (ADR-0012);<br>2. gestione dei segreti (portachiavi del sistema) per chiavi di connettori futuri;<br>3. Integration Engine: primo connettore installabile e revocabile (RF-14);<br>4. persistenza ed esportazione delle metriche |

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
| 2026-11-17 | `c2fd01f` | PR #8 unita con CI verde (desktop su 3 sistemi). Voce locale scelta dal proprietario: contratti, motori faster-whisper, Piper e openWakeWord più quelli finti, API vocali, interfaccia con microfono, risposte vocali e mani libere, `voice-setup`; corretti la CSP che bloccava l'audio e l'adattatore openWakeWord (firma 0.4 rispetto a 0.6); ADR-0010 | Python 9+33 OK, Rust 72 OK, E2E 38/38 OK | Prova del proprietario, installatore, Fase 4 |
| 2026-11-17 | `9d4a505` | PR #9 unita. Il proprietario segnala che il comando di avvio non avvia JARVIS. Cause individuate: nessun lanciatore Windows, `sh jarvis.sh` interrotto da dash, permesso di esecuzione perso con lo ZIP, prima compilazione silenziosa, browser non aperto. Interventi: `jarvis.sh` riscritto (riavvio in bash, `.venv` riparata, avanzamento della compilazione, apertura del browser, `doctor`), `jarvis.ps1` e `jarvis.cmd` per Windows, worker in UTF-8 su ogni sistema (le pipe di Windows usano cp1252), job CI `avvio` su 3 sistemi con `tests/smoke/start_smoke.py` | Avvio reale con `sh jarvis.sh` e smoke OK in locale | Esito del job `avvio`, poi Fase 4 |
| 2026-11-17 | PR #10 | Fase 4 avviata: metriche e log strutturati correlati (`observability.rs`, `/api/metrics`), pannello Sistema, iniezione di guasti nel worker, 9 test di robustezza, SLO iniziali misurati (`tests/perf`), misura in CI; ADR-0011 | Rust 85 OK, E2E 40/40 OK, p95 chat 9,1 ms | Backup e ripristino (RNF-08) |
| 2026-11-17 | PR #10 | RNF-08: backup `.jbak` (XChaCha20-Poly1305 con Argon2id), ripristino verificato (impronte, audit, integrità del database) con conservazione dei dati precedenti, blocco esclusivo della cartella dati, `jarvis backup` e `restore`, API e interfaccia; ADR-0012 | Rust 90 OK (5 backup), prova da riga di comando OK | Segreti, Integration Engine (RF-14) |

Per ogni ripresa registrare data, revisione, lavoro completato, verifiche eseguite, blocchi, ipotesi e prossimo passo concreto. Conservare il checkpoint con le modifiche alle quali si riferisce.
