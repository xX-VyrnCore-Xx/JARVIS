# Requisiti JARVIS

Gli identificativi qui assegnati servono per la tracciabilità dei test. La sequenza di rilascio è in [ROADMAP.md](ROADMAP.md).

| ID | Capacità richiesta | Verifica rappresentativa |
|---|---|---|
| RF-01 | Chat testuale e vocale con contesto autorizzato | Una conversazione riprende con il contesto pertinente |
| RF-02 | Pianificare obiettivi, dipendenze e task | Un piano produce risultati verificati e rende visibili gli errori |
| RF-03 | Coordinare dieci specializzazioni e agenti temporanei | Un task delegato rispetta scopo, strumenti e risorse concessi |
| RF-04 | Usare modelli locali e cloud mediante AI Gateway | Un errore del provider preserva lo stato e il routing rispetta la policy |
| RF-05 | Controllare file, processi, desktop e terminale | Un'azione autorizzata produce un effetto osservabile sul nodo selezionato |
| RF-06 | Ricercare, navigare e verificare fonti | Una ricerca riporta fonti e provenienza delle evidenze |
| RF-07 | Creare, pianificare e monitorare workflow | Un workflow sopravvive al riavvio e conserva la cronologia delle esecuzioni |
| RF-08 | Recuperare e aggiornare memoria persistente | L'utente può correggere o cancellare memoria e derivati |
| RF-09 | Wake word, STT/TTS in streaming, full-duplex e interruzione vocale | L'utente interrompe la risposta e vede lo stato del microfono |
| RF-10 | Desktop Windows, macOS e Linux, Web, Mobile Android/iOS e avatar evolutivo | I client previsti dalla milestone superano i rispettivi flussi E2E |
| RF-11 | Identificare e coordinare nodi autorizzati | Un dispositivo revocato perde accesso e assegnazioni |
| RF-12 | Versionare, indicizzare, sincronizzare e proteggere dati e file | Una modifica concorrente preserva le versioni fino alla risoluzione |
| RF-13 | Monitorare risorse, anomalie, attività e notifiche | Un guasto simulato è rilevabile e correlabile al task |
| RF-14 | Plugin, API, webhook, connettori e SDK | Un connettore può essere installato, autorizzato e revocato |
| RF-15 | Proporre miglioramenti controllati a codice e componenti | Una modifica passa per versionamento, prove e controlli previsti |

Lo scope operativo include produttività e documenti, email e calendario, sviluppo software con Git, build e deployment, interazione amministrativa autorizzata con servizi e database, analisi di immagini, audio, video e schermate. Ogni capacità dipende dal supporto del nodo e dai permessi per il task.

| ID | Vincolo | Evidenza |
|---|---|---|
| RNF-01 | Moduli sostituibili con contratti versionati | Prove di compatibilità tra client e servizi |
| RNF-02 | Guasti isolati e degradazione controllata | Fault injection con esiti e recupero osservabili |
| RNF-03 | Risorse adattive e Core trasferibile | Benchmark sull'hardware iniziale e prova di migrazione |
| RNF-04 | Identità, autorizzazione, audit e isolamento | Prove di accesso negato, revoca e conferma critica |
| RNF-05 | Local-first e minimizzazione dei dati | Test di blocco del traffico cloud per payload vietati |
| RNF-06 | Offline per le capacità compatibili | Test con rete assente e successiva riconciliazione |
| RNF-07 | Osservabilità e manutenibilità | Correlazione end-to-end e istruzioni di diagnosi riproducibili |
| RNF-08 | Backup, ripristino e rollback | Restore su ambiente pulito e rollback di release |

Per ogni requisito registrare milestone, componente, test, riferimento alla revisione, esito e stato. Gli stati sono Pianificato, In corso, Implementato, Verificato e Bloccato. L'accettazione finale segue [DEVOPS_QA_OBSERVABILITY.md](DEVOPS_QA_OBSERVABILITY.md).

## Tracciabilità

| ID | Milestone | Componente | Test | Revisione | Esito | Stato |
|---|---|---|---|---|---|---|
| RF-02 | Fase 1 | `contracts/v1/task.schema.json`, `task-transitions.json` | Fixture `task.*`, `TaskTransitions` | Branch `claude/cool-ramanujan-bogarl` | Python e Rust OK | In corso (contratto, non ancora esecuzione) |
| RF-03 | Fase 1 | `contracts/v1/delegation.schema.json`, `envelope.schema.json` | Fixture `delegation.*`, `envelope.*` | Idem | Python e Rust OK | In corso (contratto) |
| RF-05 | Fase 1 | `contracts/v1/tool-descriptor.schema.json` | Fixture `tool-descriptor.*` | Idem | Python e Rust OK | In corso (contratto) |
| RNF-01 | Fase 1–2 | `contracts/v1/`, `core/contracts`, `core/bridge`, `ai/jarvis_ai` | Stesse fixture in Python e Rust; scambio reale di envelope fra processi | Idem | Esiti identici su 14 fixture valide e 24 non valide; round trip validato da entrambi i lati | In corso |
| RNF-04 | Fase 1 | `policy-decision.schema.json`, `audit-event.schema.json` | Fixture `policy-decision.*`, `audit-event.*`, catene `streams/` | Idem | Modifica e cancellazione di un evento rilevate | In corso (contratto) |
| RNF-07 | Fase 1 | Correlazione in `envelope`, `task`, `audit-event` | Fixture comuni | Idem | OK | In corso (contratto) |
| RNF-02 | Fase 2 | `core/bridge` | `timeout_kills_worker_and_reports_unknown_effect`, `worker_crash_is_detected`, `malformed_response_is_rejected` | Idem | Guasti del worker isolati e riportati come `error` v1 | In corso (prototipo) |
| RF-03 | Fase 2 | `core/bridge`, `ai/jarvis_ai/worker.py` | `delegation_round_trip_is_valid_and_correlated`, `tests/ai/test_worker.py` | Idem | Delega consegnata e risposta correlata | In corso (agente sintetico) |
| RF-01 | Fase 2 (v0.1) | Chat in `core/server`, contesto in `store.rs` | `memory_round_trip_and_owner_deletion`, E2E | Branch `claude/cool-ramanujan-bogarl` | Contesto e memoria persistono dopo un riavvio | In corso (solo testo; voce assente) |
| RF-04 | Fase 2 (v0.1) | `ai/jarvis_ai/gateway.py` | `tests/ai/test_gateway.py` (Ollama sintetico, degradazione) | Idem | Routing locale e degradazione dichiarata | In corso (Ollama reale non verificato) |
| RF-05 | Fase 2 (v0.1) | `tools.rs`, `policy.rs` | `low_risk_tool_runs_without_confirmation`, `high_risk_write_requires_single_use_confirmation` | Idem | Effetto osservabile nella workspace | In corso (file e informazioni di sistema) |
| RF-08 | Fase 2 (v0.1) | Memoria SQLite e interfaccia Memoria | `memory_round_trip_and_owner_deletion`, E2E | Idem | Il proprietario corregge e cancella | In corso (nessun indice derivato) |
| RNF-04 | Fase 2 (v0.1) | `api.rs`, `policy.rs`, `audit.rs` | `api_requires_token_and_local_host`, `path_traversal_*`, `symlink_*`, `audit_chain_records_every_decision` | Idem | Accesso negato, conferma monouso, manomissione rilevata | In corso (manca l'identità forte) |
| RNF-05 | Fase 2 (v0.1) | Delega con `allow_cloud: false`, nessun backend cloud | `OllamaAdapter` (solo URL locale) | Idem | Nessun traffico cloud | In corso |
| RNF-06 | Fase 2 (v0.1) | Modello offline | `OfflineBackend`, test API ed E2E senza rete | Idem | Funzionamento completo senza rete | In corso |
| RF-07 | Fase 3 | `core/server/src/automation.rs`, `orchestrator.rs`, sezione Automazioni dell'interfaccia | `core/server/tests/automation.rs` (10), unitari di `automation.rs` (5), intenti in `test_gateway.py`, E2E | Branch `claude/cool-ramanujan-bogarl` | Il workflow sopravvive al riavvio, recupera un'esecuzione persa e conserva la cronologia; la policy si applica in background; i passi alterati sono rilevati | Implementato; modifica con versioni (ADR-0008); notifiche in RF-13 |
| RF-13 | Fase 3 | Notifiche e monitor nel Core, campanella dell'interfaccia | `core/server/tests/notifications.rs` (7), E2E | Branch `claude/cool-ramanujan-bogarl` | Guasti simulati (agente assente, automazione fallita, crash del worker, audit alterato) notificati e correlati a task e workflow | Implementato (mancano le soglie su risorse e i canali esterni) |
| RF-06 | Fase 3 | `core/server/src/web.rs`, policy Web, fonti nell'interfaccia | `core/server/tests/web.rs` (8), unitari in `web.rs` (3), `test_gateway.py`, E2E | Branch `claude/cool-ramanujan-bogarl` | Ricerca e lettura riportano fonti e provenienza (URL, ora, SHA-256); SSRF e iniezione bloccate | Implementato (prova sul Web reale sospesa; navigazione interattiva non inclusa) |
| RF-10 | Fase 3 | Desktop: `apps/desktop` (Tauri e launcher) | `apps/desktop/launcher/tests/launch.rs` (4), CI desktop su 3 sistemi, prova con Xvfb | Branch `claude/cool-ramanujan-bogarl` | Finestra con l'interfaccia del Core; nessun processo orfano dopo un crash | In corso (desktop senza installatore; Web completato; mobile e avatar da fare) |
| RF-09 | Fase 3 | `ai/jarvis_ai/voice.py`, `/api/voice/*`, controlli vocali nell'interfaccia | `core/server/tests/voice.rs` (6), `tests/ai/test_voice.py` (10), E2E (8 verifiche vocali) | Branch `claude/cool-ramanujan-bogarl` | Microfono visibile, trascrizione, risposta vocale interrompibile, mani libere con «Ehi Jarvis» | Implementato con motori finti in CI; riconoscimento e sintesi reali da verificare dal proprietario |
| RNF-02 | Fase 4 | Iniezione di guasti nel worker, isolamento del bridge, monitor | `core/server/tests/resilience.rs` (9) | Branch `claude/cool-ramanujan-bogarl` | Guasti rilevati, correlati, isolati e recuperati | In corso (manca la perdita di rete fra nodi) |
| RNF-07 | Fase 4 | `observability.rs`, `/api/metrics`, `data/logs/core.jsonl`, sezione Sistema | `resilience.rs`, test unitari, E2E, `tests/perf/latency.py` | Idem | Correlazione end-to-end senza contenuti, latenze misurate, storico di 7 giorni esportabile (JSON e Prometheus) | In corso (mancano le trace distribuite) |
| RF-14 | Fase 4 | `core/server/src/connectors.rs`, `secrets.rs`, `/api/connectors`, `/api/hooks/{id}`, sezione Connettori | `core/server/tests/connectors.rs` (4), unitari (4), fixture `connector.*`, E2E | Branch `claude/cool-ramanujan-bogarl` | Un connettore viene installato, autorizzato con credenziale cifrata, usato con conferma delle scritture e revocato con effetto immediato; webhook firmati | Implementato (connettori dichiarativi HTTP; mancano OAuth, SDK e plugin con codice) |
| RF-11 | Fase 5 | Dispositivi associati in `orchestrator.rs` e `api.rs`, pagina `/pair`, sezione Dispositivi | `core/server/tests/devices.rs` (4), fixture `device.*`, E2E con host remoto | Branch `claude/cool-ramanujan-bogarl` | Un dispositivo revocato perde subito l'accesso; permessi minimi; associazione monouso | In corso (dispositivi client; mancano nodi esecutori, mTLS e VPN mesh) |
| RNF-08 | Fase 4 | `core/server/src/backup.rs`, `jarvis backup` e `jarvis restore`, `POST /api/backup` | `core/server/tests/backup.rs` (5), E2E, prova da riga di comando | Branch `claude/cool-ramanujan-bogarl` | Ripristino su ambiente pulito verificato; alterazioni e passphrase errate rifiutate senza effetti | Implementato: backup cifrato, ripristino verificato e rollback di release (`rollback.rs`, ADR-0015); mancano gli installatori firmati |
