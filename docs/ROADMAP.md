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
| Ultima revisione verificata | `f5aba7b` sul branch `claude/cool-ramanujan-bogarl`, con CI verde (job 134038010136). Le modifiche di questa ripresa sono nel commit successivo dello stesso branch, in [PR #1](https://github.com/xX-VyrnCore-Xx/JARVIS/pull/1) |
| Milestone effettiva | Fase 1 — condizione di uscita raggiunta (contratti iniziali verificabili, dipendenze documentate in [contracts/README.md](../contracts/README.md)), in attesa di accettazione del proprietario. Fase 2 — Prototipi tecnici, in corso |
| Risultato disponibile | Contratti v1: common, envelope, error, delegation, tool-descriptor, task (+ tabella transizioni), policy-decision, audit-event. Crate Rust `jarvis-contracts` in `core/contracts` con gli stessi schemi e invarianti (ADR-0001, ADR-0002 in [DECISIONS.md](DECISIONS.md)). Tracciabilità in [REQUIREMENTS.md](REQUIREMENTS.md) |
| Verifiche del software | Container remoto Linux: `python3 -m unittest discover -s tests/contracts -v` (Python 3.13.18, jsonschema 4.26.0) — 8 test OK; `cargo test --locked` in `core/` (Rust 1.97.0) — 5 test OK; `cargo clippy --all-targets -- -D warnings` e `cargo fmt --check` OK. Esiti identici Python/Rust su 14 fixture valide e 24 non valide, catena di audit valida, manomessa e con evento cancellato. CI GitHub: job Python verde su `f5aba7b`; job Rust aggiunto in questa ripresa, esito da leggere sulla PR |
| Lacune documentali | Risposte 75–80 senza fonte, lasciate vuote. Scelte aperte invariate: RPC/event bus, storage, segreti, VPN mesh, motori vocali, stack mobile, SLO. Firma o ancoraggio esterno della catena di audit da decidere con la gestione dei segreti |
| Blocchi | Nessuno tecnico. Il blocco di GitHub Actions è risolto dopo che il repository è stato reso pubblico. `origin/main` resta una cronologia non correlata senza merge base: integrazione da decidere con il proprietario. MSRV 1.80 dichiarata ma non verificata |
| Prossima azione | Fase 2, integrazione Rust/Python: prototipo di bridge fra Core Rust e worker Python con messaggi `jarvis.envelope/1` in JSON Lines su stdio. Il Core avvia il worker, invia una `delegation`, riceve l'esito o un `error`, applica il timeout e chiude il processo; entrambi i lati validano con i contratti v1. Il trasporto definitivo (RPC) resta aperto: il bridge è una prova, non una scelta |

### Registro delle riprese

| Data | Revisione di partenza | Lavoro | Verifiche | Prossimo passo |
|---|---|---|---|---|
| 2026-11-17 | `ad1972c` | Inventario: solo documentazione in `docs/`. Creati contratti v1, fixture sintetiche, test, CI, `CLAUDE.md` in radice, ADR-0001 | 4 test unittest OK | Contratti `task` e `policy-decision` |
| 2026-11-17 | `f5aba7b` | CI rieseguita e verde dopo che il repository è stato reso pubblico. Contratti task, policy-decision e audit-event; matrice delle dipendenze; chiusura tecnica della Fase 1. Fase 2: crate Rust `jarvis-contracts`, job CI Rust, ADR-0002, tracciabilità dei requisiti | Python 8 OK, Rust 5 OK, clippy e fmt OK | Bridge Rust–Python su stdio |

Per ogni ripresa registrare data, revisione, lavoro completato, verifiche eseguite, blocchi, ipotesi e prossimo passo concreto. Conservare il checkpoint con le modifiche alle quali si riferisce.
