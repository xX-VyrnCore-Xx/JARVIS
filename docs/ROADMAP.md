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

| Campo | Stato al 2026-11-17 (quarta ripresa) |
|---|---|
| Ultima revisione verificata | Base `5330b3c` su `main`: la PR #3 (prototipo v0.1) è stata unita con CI verde. Il lavoro di questa ripresa è sul branch `claude/cool-ramanujan-bogarl` in una nuova PR, da unire automaticamente con CI verde |
| Milestone effettiva | **Fase 3 — MVP avanzato, in corso.** Completato RF-07 Automation Engine (ADR-0005) |
| Risultato disponibile | Prototipo v0.1 più le automazioni:<br>• contratti `jarvis.workflow/1` e `jarvis.workflow-run/1`;<br>• strumenti `workflow.create`, `list`, `run`, `delete`;<br>• pianificatore nel Core;<br>• creazione da chat («ogni ora info sistema») e dalla sezione Automazioni;<br>• approvazione legata al digest dei passi, policy rivalutata in background, cronologia delle esecuzioni, sopravvivenza al riavvio con recupero di una sola esecuzione persa |
| Verifiche del software | Container remoto Linux, Python 3.13.18, Rust 1.97.0, Chromium (Playwright 1.63.0).<br>• `python3 -m unittest discover -s tests/contracts -v`: 9 test OK (fixture valide e non valide per workflow e workflow-run);<br>• `python3 -m unittest discover -s tests/ai -v`: 19 test OK;<br>• `cargo test --locked` in `core/`: 38 test OK (5 contratti, 6 bridge, 5 unitari delle automazioni, 12 API, 10 automazioni);<br>• `cargo clippy --all-targets -- -D warnings` e `cargo fmt --check` OK;<br>• `tests/e2e/ui_smoke.cjs`: 17 verifiche OK;<br>• prova manuale: un'automazione `fs.write` ogni minuto, creata via API su un server reale, è stata eseguita dal pianificatore in background |
| Controlli sospesi | Ollama con un modello reale: non installabile nel container. Da verificare sul computer del proprietario, compresa la generazione degli argomenti di `workflow.create` da parte del modello |
| Lacune documentali | Risposte 75–80 senza fonte, lasciate vuote. Restano aperte: RPC ed event bus, PostgreSQL, segreti, VPN mesh, motori vocali, stack mobile, SLO, firma dell'audit. Deviazioni temporanee: interfaccia senza React e Tauri, SQLite (ADR-0004) |
| Blocchi | Nessuno |
| Prossima azione | Fase 3, nell'ordine:<br>1. RF-13: notifiche per gli esiti delle automazioni e per le conferme in attesa (stato nell'interfaccia, base per desktop e mobile);<br>2. RF-06: strumenti `web.search` e `web.fetch` con citazione delle fonti e allowlist di dominio nella policy;<br>3. modifica in loco di un'automazione con nuova approvazione (versione + 1);<br>4. shell desktop Tauri |

### Registro delle riprese

| Data | Revisione di partenza | Lavoro | Verifiche | Prossimo passo |
|---|---|---|---|---|
| 2026-11-17 | `ad1972c` | Inventario: solo documentazione in `docs/`. Creati contratti v1, fixture sintetiche, test, CI, `CLAUDE.md` in radice, ADR-0001 | 4 test unittest OK | Contratti `task` e `policy-decision` |
| 2026-11-17 | `f5aba7b` | CI rieseguita e verde dopo che il repository è stato reso pubblico. Contratti task, policy-decision e audit-event; matrice delle dipendenze; chiusura tecnica della Fase 1. Fase 2: crate Rust `jarvis-contracts`, job CI Rust, ADR-0002, tracciabilità dei requisiti | Python 8 OK, Rust 5 OK, clippy e fmt OK | Bridge Rust–Python su stdio |
| 2026-11-17 | `d482b80` | Branch ricreato da `main` dopo il merge della PR #1. Corretti i collegamenti di `CLAUDE.md` e `README.md`, spostati in radice dal proprietario. Validazione Python spostata in `ai/jarvis_ai/contracts.py`. Bridge stdio (`core/bridge`, `ai/jarvis_ai/worker.py`), ADR-0003, CI estesa | Python 8+6 OK, Rust 11 OK, clippy e fmt OK | AI Gateway con adapter sintetico e adapter Ollama |
| 2026-11-17 | `795986d` | PR #2 unita con l'approvazione del proprietario; autorizzato il merge automatico con CI verde. Prototipo integrato v0.1: contratto `agent-result`, catalogo degli strumenti, AI Gateway (Ollama e offline), Core HTTP con policy, conferme, SQLite, audit e interfaccia Web, `jarvis.sh`, test API ed E2E, job CI E2E, ADR-0004 e specifiche aggiornate | Python 9+17 OK, Rust 23 OK, E2E 12/12 OK | Riscontro del proprietario con Ollama reale; Automation Engine (RF-07) |
| 2026-11-17 | `5330b3c` | PR #3 unita con CI verde. Fase 3, RF-07: contratti workflow e workflow-run, strumenti `workflow.*`, Automation Engine nel Core con pianificatore, approvazione legata al digest dei passi, policy in background, recupero dopo riavvio, cronologia; intenti offline di pianificazione; sezione Automazioni dell'interfaccia; ADR-0005; percorso Python relativo risolto all'avvio | Python 9+19 OK, Rust 38 OK, E2E 17/17 OK, pianificatore reale verificato | RF-13 notifiche, poi RF-06 Web |

Per ogni ripresa registrare data, revisione, lavoro completato, verifiche eseguite, blocchi, ipotesi e prossimo passo concreto. Conservare il checkpoint con le modifiche alle quali si riferisce.
