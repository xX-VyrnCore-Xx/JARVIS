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

| Campo | Stato al 2026-11-17 (terza ripresa) |
|---|---|
| Ultima revisione verificata | Base `795986d` su `main`: la PR #2 è stata unita dopo la CI verde, con l'approvazione del proprietario. Il lavoro di questa ripresa è sul branch `claude/cool-ramanujan-bogarl` in una nuova PR, che verrà unita automaticamente con CI verde |
| Milestone effettiva | Fase 1 accettata dal proprietario. Fase 2 — Prototipi tecnici: **prototipo integrato v0.1 testabile** (ADR-0004). Il passo successivo apre la Fase 3 (MVP avanzato) |
| Risultato disponibile | `./jarvis.sh` avvia il Core (`core/server`, binario `jarvis`) con interfaccia Web su `http://localhost:8765`. Comprende chat, agente General Assistant (worker Python), AI Gateway con Ollama o modello offline, 8 strumenti a catalogo, policy con conferma monouso, memoria SQLite, task persistenti e audit a catena di hash. Istruzioni nel [README](../README.md#prova-il-prototipo-v01) |
| Verifiche del software | Container remoto Linux, Python 3.13.18, Rust 1.97.0, Chromium (Playwright 1.63.0). `python3 -m unittest discover -s tests/contracts -v`: 9 test OK. `python3 -m unittest discover -s tests/ai -v`: 17 test OK. `cargo test --locked` in `core/`: 23 test OK (5 contratti, 6 bridge, 12 API con worker reale). `cargo clippy --all-targets -- -D warnings` e `cargo fmt --check` OK. `tests/e2e/ui_smoke.cjs`: 12 verifiche OK, desktop e 390 px, nessun errore CSP. Avvio da zero con `./jarvis.sh` verificato |
| Controlli sospesi | Ollama con un modello reale: non installabile nel container (download bloccato dal proxy). L'adapter è provato solo su un server sintetico che imita l'API. Da verificare sul computer del proprietario: `ollama pull qwen2.5:3b`, poi `./jarvis.sh` |
| Lacune documentali | Risposte 75–80 senza fonte, lasciate vuote. Restano aperte: RPC ed event bus, storage centrale (PostgreSQL), segreti, VPN mesh, motori vocali, stack mobile, SLO, firma dell'audit. Deviazioni temporanee: interfaccia senza React e Tauri, SQLite al posto di PostgreSQL (ADR-0004) |
| Blocchi | Nessuno |
| Prossima azione | Fase 3, nell'ordine: (1) raccogliere il riscontro del proprietario sulla v0.1 con Ollama reale e correggere il tool calling; (2) RF-07: Automation Engine con workflow pianificati che sopravvivono al riavvio, con cronologia delle esecuzioni e policy applicata in background; (3) RF-06: strumento `web.search`/`web.fetch` con citazione delle fonti; (4) shell desktop Tauri sopra l'interfaccia attuale |

### Registro delle riprese

| Data | Revisione di partenza | Lavoro | Verifiche | Prossimo passo |
|---|---|---|---|---|
| 2026-11-17 | `ad1972c` | Inventario: solo documentazione in `docs/`. Creati contratti v1, fixture sintetiche, test, CI, `CLAUDE.md` in radice, ADR-0001 | 4 test unittest OK | Contratti `task` e `policy-decision` |
| 2026-11-17 | `f5aba7b` | CI rieseguita e verde dopo che il repository è stato reso pubblico. Contratti task, policy-decision e audit-event; matrice delle dipendenze; chiusura tecnica della Fase 1. Fase 2: crate Rust `jarvis-contracts`, job CI Rust, ADR-0002, tracciabilità dei requisiti | Python 8 OK, Rust 5 OK, clippy e fmt OK | Bridge Rust–Python su stdio |
| 2026-11-17 | `d482b80` | Branch ricreato da `main` dopo il merge della PR #1. Corretti i collegamenti di `CLAUDE.md` e `README.md`, spostati in radice dal proprietario. Validazione Python spostata in `ai/jarvis_ai/contracts.py`. Bridge stdio (`core/bridge`, `ai/jarvis_ai/worker.py`), ADR-0003, CI estesa | Python 8+6 OK, Rust 11 OK, clippy e fmt OK | AI Gateway con adapter sintetico e adapter Ollama |
| 2026-11-17 | `795986d` | PR #2 unita con l'approvazione del proprietario; autorizzato il merge automatico con CI verde. Prototipo integrato v0.1: contratto `agent-result`, catalogo degli strumenti, AI Gateway (Ollama e offline), Core HTTP con policy, conferme, SQLite, audit e interfaccia Web, `jarvis.sh`, test API ed E2E, job CI E2E, ADR-0004 e specifiche aggiornate | Python 9+17 OK, Rust 23 OK, E2E 12/12 OK | Riscontro del proprietario con Ollama reale; Automation Engine (RF-07) |

Per ogni ripresa registrare data, revisione, lavoro completato, verifiche eseguite, blocchi, ipotesi e prossimo passo concreto. Conservare il checkpoint con le modifiche alle quali si riferisce.
