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

| Campo | Stato al 2026-11-17 (seconda ripresa) |
|---|---|
| Ultima revisione verificata | Base `d482b80` su `main`: la PR #1 è stata unita dal proprietario. Il lavoro successivo è sul branch `claude/cool-ramanujan-bogarl`, ricreato da `main`, in una nuova PR. CI verde anche su `2187b8e` (job Python e Rust), contenuto ora riportato sul nuovo branch |
| Milestone effettiva | Fase 1 — condizione di uscita raggiunta: contratti iniziali verificabili e dipendenze documentate in [contracts/README.md](../contracts/README.md). Manca l'accettazione del proprietario. Fase 2 — Prototipi tecnici, in corso |
| Risultato disponibile | Contratti v1: common, envelope, error, delegation, tool-descriptor, task (+ tabella delle transizioni), policy-decision, audit-event. Crate Rust `jarvis-contracts` (`core/contracts`). Bridge prototipale Core–worker Python su stdio: crate `jarvis-bridge` (`core/bridge`) e `ai/jarvis_ai/worker.py`. ADR-0001…0003 in [DECISIONS.md](DECISIONS.md), tracciabilità in [REQUIREMENTS.md](REQUIREMENTS.md) |
| Verifiche del software | Container remoto Linux, Python 3.13.18 con jsonschema 4.26.0, Rust 1.97.0. `python3 -m unittest discover -s tests/contracts -v`: 8 test OK. `python3 -m unittest discover -s tests/ai -v`: 6 test OK. `cargo test --locked` in `core/`: 11 test OK (5 contratti, 6 bridge con worker reale: round trip, errore dell'agente, timeout con terminazione, crash, risposta malformata, delega non valida). `cargo clippy --all-targets -- -D warnings` e `cargo fmt --check` OK. CI della nuova PR da leggere su GitHub |
| Lacune documentali | Risposte 75–80 senza fonte, lasciate vuote. Restano aperte: RPC ed event bus (il bridge stdio è un prototipo), storage, segreti, VPN mesh, motori vocali, stack mobile, SLO, firma o ancoraggio della catena di audit |
| Blocchi | Nessuno. Risolti: GitHub Actions (repository reso pubblico) e cronologia non correlata di `main` (sostituita dal merge della PR #1). Non verificati: MSRV 1.80; Ollama assente nel container, quindi le prove con un runtime AI reale restano sospese |
| Prossima azione | Fase 2, runtime AI: contratti `ai-request` e `ai-response` (chat, streaming a chunk, errori) e un AI Gateway Python con adapter sintetico e adapter Ollama (HTTP su `localhost:11434`). Routing local-first che nega l'escalation cloud quando la delega ha `allow_cloud: false` (RF-04, RNF-05). Provare l'adapter Ollama solo dove il servizio è disponibile e registrare il controllo sospeso altrove |

### Registro delle riprese

| Data | Revisione di partenza | Lavoro | Verifiche | Prossimo passo |
|---|---|---|---|---|
| 2026-11-17 | `ad1972c` | Inventario: solo documentazione in `docs/`. Creati contratti v1, fixture sintetiche, test, CI, `CLAUDE.md` in radice, ADR-0001 | 4 test unittest OK | Contratti `task` e `policy-decision` |
| 2026-11-17 | `f5aba7b` | CI rieseguita e verde dopo che il repository è stato reso pubblico. Contratti task, policy-decision e audit-event; matrice delle dipendenze; chiusura tecnica della Fase 1. Fase 2: crate Rust `jarvis-contracts`, job CI Rust, ADR-0002, tracciabilità dei requisiti | Python 8 OK, Rust 5 OK, clippy e fmt OK | Bridge Rust–Python su stdio |
| 2026-11-17 | `d482b80` | Branch ricreato da `main` dopo il merge della PR #1. Corretti i collegamenti di `CLAUDE.md` e `README.md`, spostati in radice dal proprietario. Validazione Python spostata in `ai/jarvis_ai/contracts.py`. Bridge stdio (`core/bridge`, `ai/jarvis_ai/worker.py`), ADR-0003, CI estesa | Python 8+6 OK, Rust 11 OK, clippy e fmt OK | AI Gateway con adapter sintetico e adapter Ollama |

Per ogni ripresa registrare data, revisione, lavoro completato, verifiche eseguite, blocchi, ipotesi e prossimo passo concreto. Conservare il checkpoint con le modifiche alle quali si riferisce.
