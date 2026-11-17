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

| Campo | Stato al 2026-11-17 |
|---|---|
| Ultima revisione verificata | Base `ad1972c` sul branch `claude/cool-ramanujan-bogarl`; le modifiche di questa sessione sono nel commit successivo dello stesso branch |
| Milestone effettiva | Fase 1 — Architettura e contratti, in corso |
| Risultato disponibile | Contratti v1 in `contracts/v1/`: common, envelope, error, delegation, tool-descriptor (RF-03, RF-05, RNF-01, RNF-04, RNF-07). Vedi [contracts/README.md](../contracts/README.md) e ADR-0001 in [DECISIONS.md](DECISIONS.md) |
| Verifiche del software | `python3 -m unittest discover -s tests/contracts -v` — Python 3.13.18, jsonschema 4.26.0, container Linux remoto — 4 test OK (7 fixture valide, 11 non valide, ciascuna respinta dalla sola regola prevista). Workflow CI `.github/workflows/contracts.yml`: run 44082465183 fallita senza runner assegnato (nessun log, 2 s), quindi blocco dell'account GitHub Actions e non dei test |
| Lacune documentali | Risposte 75–80 senza fonte, lasciate vuote. Scelte aperte invariate: RPC/event bus, storage, segreti, VPN mesh, motori vocali, stack mobile, SLO |
| Blocchi | Nessuno per la Fase 1. `origin/main` ha una cronologia non correlata (piano in inglese, `docs/PLAN.md`) senza merge base con questo branch: integrazione da decidere con il proprietario. GitHub Actions non avvia i job (verificare in Settings → Billing il limite di spesa o i minuti per i repository privati, e in Settings → Actions che siano abilitate) |
| Prossima azione | Contratti `task` (stato e transizioni del task: RF-02) e `policy-decision` (decisione del Security Engine con audit: RNF-04), con fixture e test nello stesso schema. Poi workspace Cargo in `core/` che carica e valida i contratti v1 |

### Registro delle riprese

| Data | Revisione di partenza | Lavoro | Verifiche | Prossimo passo |
|---|---|---|---|---|
| 2026-11-17 | `ad1972c` | Inventario: solo documentazione in `docs/`. Creati contratti v1, fixture sintetiche, test, CI, `CLAUDE.md` in radice, ADR-0001 | 4 test unittest OK | Contratti `task` e `policy-decision` |

Per ogni ripresa registrare data, revisione, lavoro completato, verifiche eseguite, blocchi, ipotesi e prossimo passo concreto. Conservare il checkpoint con le modifiche alle quali si riferisce.
