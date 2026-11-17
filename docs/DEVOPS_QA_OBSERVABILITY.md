# DevOps, QA e osservabilità

## Build e distribuzione

Il Core Linux combina servizi systemd e componenti Docker. Il progetto prevede installer multipiattaforma e aggiornamenti per modulo con verifica crittografica, canali Development, Beta e Stable, health check e rollback automatico. Ogni release registra commit, versioni dei contratti, artefatti, prerequisiti e compatibilità dei dati. Verificare in ambiente di prova upgrade, compatibilità con client già distribuiti e recupero dopo migrazione fallita.

## Verifiche

| Livello | Copertura |
|---|---|
| Unitario | Policy, routing, transizioni, conflitti e funzioni dei moduli |
| Contratti e integrazione | Compatibilità API/eventi, Rust–Python, database, provider e sincronizzazione |
| E2E | Chat, piano, azione autorizzata, voce, Web, workflow e client previsti |
| Multipiattaforma | Installazione, permessi OS, aggiornamento e compatibilità |
| Prestazioni | Latenze, risorse, carichi agentici, qualità offline e SLO |
| Sicurezza e privacy | Accesso minimo, isolamento, injection, revoca, egress e retention |
| Guasti | Fault injection, chaos engineering, riavvio, perdita di rete e provider |
| Ripristino | Restore da backup cifrato, integrità dei dati e rollback |

## Diagnosi

Correlare richiesta, task, agente, provider, strumento e nodo con identificativi condivisi. Rilevare salute dei servizi, profondità e attesa delle code, errori, latenza, risorse dei runtime, sincronizzazione e stato degli aggiornamenti. Documentare soglie e frequenza di misurazione quando i carichi di riferimento sono definiti. Minimizzare telemetria e payload come da [SECURITY_AND_PRIVACY.md](SECURITY_AND_PRIVACY.md).

## Enterprise Acceptance System

Per l'accettazione finale raccogliere la matrice requisiti–test con esiti, la compatibilità multipiattaforma, la validazione di sicurezza e privacy, le misure di prestazione rispetto agli SLO, le prove di fault tolerance, restore e rollback, la documentazione e le evidenze di deploy e aggiornamento. Registrare eventuali difetti aperti e verificare l'assenza di problemi critici o bloccanti. La chiusura richiede l'accettazione esplicita del proprietario. Valori numerici per SLO e obiettivi di recovery richiedono definizione con evidenze prima della relativa verifica.

## Stato della CI (prototipo v0.1)

Il workflow `.github/workflows/contracts.yml` esegue tre job:
- **Python:** contratti e AI Gateway;
- **Rust:** fmt, clippy e test, compresi quelli d'integrazione sull'API con il worker reale;
- **E2E:** Core in modalità offline più Chromium (`tests/e2e/ui_smoke.cjs`), con gli screenshot pubblicati come artefatto.

Il proprietario ha autorizzato il merge automatico con CI verde ([DEVELOPMENT.md](DEVELOPMENT.md#integrazione-delle-modifiche)).

## Notifiche e monitor (Fase 3, RF-13)

Il Core trasforma eventi e anomalie in notifiche `jarvis.notification/1`, correlate al task e al workflow che le hanno prodotte.

| Origine | Gravità | Deduplicazione |
|---|---|---|
| Conferma in attesa | warning; si chiude quando viene risolta | per conferma |
| Conferma scaduta | info | — |
| Agente AI non disponibile | error | finché non letta |
| AI degradata (Ollama assente o in errore) | warning | finché non letta |
| Automazione fallita o parziale | error / warning | per workflow, finché non letta |
| Automazione sospesa per passi alterati | error (sicurezza) | — |
| Registro di audit alterato | error (sicurezza) | finché non letta |
| Worker AI terminato | warning | finché non letta |

Il monitor gira a ogni intervallo del pianificatore. Le notifiche si consultano da `GET /api/notifications` e si segnano come lette con `POST /api/notifications/{id}/read` o `POST /api/notifications/read-all`.

## Avvio verificato per sistema operativo

Il job CI `avvio` esegue i lanciatori reali (`jarvis.sh` su Linux e macOS, `jarvis.ps1` su Windows) da un checkout pulito. Poi `tests/smoke/start_smoke.py` attende il Core e verifica health, chat con caratteri accentati e uno strumento eseguito tramite policy. I lanciatori offrono `doctor` per la diagnosi dei prerequisiti.

## Osservabilità e robustezza (Fase 4, ADR-0011)

**Metriche:** `GET /api/metrics` riporta conteggio, errori e latenze p50, p95 e max per endpoint, strumento, delega, task e automazione, più i contatori del worker. La sezione «Sistema» dell'interfaccia le mostra in tabella.

**Storico ed esportazione:** un'istantanea ogni 5 minuti e all'arresto ordinato, conservata 7 giorni in SQLite. `GET /api/metrics/history?hours=24` restituisce lo storico; `GET /api/metrics/export` lo scarica in JSON (`jarvis-metrics/1`) e `?format=prometheus` nel formato testuale di Prometheus. Le richieste vogliono l'header `X-Jarvis-Token`, il token stampato nella pagina dell'interfaccia: per uno scraper Prometheus locale va configurato come header della richiesta. Esempio:

```bash
curl -H "X-Jarvis-Token: <token>" "http://127.0.0.1:8765/api/metrics/export?format=prometheus"
```

**Log:** `data/logs/core.jsonl` (JSON Lines, rotazione a 5 MiB) contiene eventi correlati (`task_id`, `correlation_id`, durata, codice d'errore) e non contiene messaggi, file né audio dell'utente.

**Iniezione di guasti** (solo con `JARVIS_FAULT_INJECTION=1`):

| Guasto | Effetto atteso | Verifica |
|---|---|---|
| `hang` | Task `failed` con `bridge.timeout`, effetto `unknown`, notifica correlata; worker riavviato alla richiesta successiva | `worker_hang_times_out_and_recovers` |
| `crash` | `bridge.worker_unavailable`, poi recupero | `worker_crash_is_isolated_and_recovers` |
| `garbage` | `bridge.invalid_response`, poi recupero | `worker_garbage_output_is_rejected_and_recovers` |
| `slow:<ms>` | Successo, latenza registrata | `slow_worker_within_timeout_is_measured_not_failed` |
| Audit cancellato | Catena non valida e notifica di sicurezza | `deleted_audit_log_is_detected_and_core_keeps_working` |
| Workspace rimossa | Errore dello strumento; il resto continua | `removed_workspace_degrades_to_a_tool_error` |

**Obiettivi di latenza iniziali** (`tests/perf/slo.json`, percorso offline, p95):

| Scenario | Obiettivo | Misura locale |
|---|---|---|
| health | 500 ms | 3,4 ms |
| elenco task | 200 ms | 1,7 ms |
| chat senza strumenti | 1000 ms | 9,1 ms |
| chat con strumento | 1500 ms | 15,3 ms |

Gli obiettivi sono volutamente larghi per i runner CI: servono a rilevare regressioni gravi. Quelli con modelli reali si fissano dopo le misure sull'hardware del proprietario.

## Release e rollback (ADR-0015)

- `python scripts/package.py --binary core/target/release/jarvis --target linux-x86_64 --out dist` crea l'archivio e il file `.sha256`.
- `.github/workflows/release.yml` costruisce e prova gli archivi sui 3 sistemi; con un tag `vX.Y.Z` prepara una bozza di release.
- All'avvio di una versione diversa il Core copia i dati in `<dati>-rollback/`; `./jarvis.sh rollback` ripristina l'ultima copia.
- Checklist di accettazione e criteri di rilascio: [ACCEPTANCE.md](ACCEPTANCE.md).
