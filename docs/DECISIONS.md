# Registro delle decisioni JARVIS

## Fonti

- [Pianificazione JARVIS](chatgpt-conversation://6afc1f2e-6f2c-83eb-9224-7f70b98eeb6d) contiene il piano complessivo e l'approvazione esplicita del proprietario, oltre alle risposte direttamente leggibili 81–83.
- [Pianificazione JARVIS (1)](chatgpt-conversation://6afc3d55-3e3c-83eb-b173-6a9d83fd1bd7) contiene la stesura documentale precedente. La parte leggibile è stata usata per dettagliare le sezioni di competenza.
- [Nomi repository JARVIS](chatgpt-conversation://6af62a95-e830-83eb-a359-5fb3d33c4ab1) attesta le scelte iniziali su esperienza desktop, AI ibrida, orchestrazione centrale e funzionamento offline parziale.

## Risposte numerate verificabili

| Numero | Risposta | Decisione |
|---|---|---|
| 75 | Fonte non recuperata | |
| 76 | Fonte non recuperata | |
| 77 | Fonte non recuperata | |
| 78 | Fonte non recuperata | |
| 79 | Fonte non recuperata | |
| 80 | Fonte non recuperata | |
| 81 | D | Ecosistema universale fin dall'inizio. Framework completo con catalogo iniziale prioritario ed espansione progressiva |
| 82 | D | Enterprise Acceptance System. Verifiche formali e accettazione esplicita del proprietario |
| 83 | D | Autonomous Personal AI Ecosystem. Identità definitiva del prodotto |

Per 81–83 il proprietario ha confermato l'opzione D senza precisazioni. Per 1–74 la corrispondenza tra numerazione storica e decisioni tecniche non è stata ricostruita integralmente. Le scelte del piano approvato sono riportate per argomento.

## Scelte consolidate dal piano approvato

| Argomento | Decisione | Specifica |
|---|---|---|
| Topologia | Core Linux, architettura ibrida modulare, nodi distribuiti e offline per le capacità compatibili | [Architettura](ARCHITECTURE.md) |
| AI | Orchestrator, dieci agenti, deleghe temporanee, Ollama, llama.cpp e Ollama Cloud su escalation | [Sistema AI](AI_SYSTEM.md) |
| Stack | Rust, Python, Tauri/React/TypeScript, Playwright, systemd e Docker | [Stack](TECH_STACK.md) |
| Dati | PostgreSQL, SQLite, pgvector, Qdrant, knowledge graph, storage documentale e a oggetti, sync bidirezionale | [Memoria e dati](MEMORY_DATA_SYNC.md) |
| Autonomia | Operazioni entro permessi e rischio, workflow e miglioramenti controllati | [Autonomia](AUTONOMY_AND_TOOLS.md) |
| Sicurezza | Zero Trust, passkeys e MFA, mTLS, sandbox, audit e privacy by design | [Sicurezza](SECURITY_AND_PRIVACY.md) |
| Esperienza | Desktop, Web, Mobile, voce full-duplex e avatar evolutivo | [Interfacce](INTERFACE_AND_DEVICES.md) |
| Release | Canali Stable, Beta e Development, verifica crittografica, rollback, QA e recovery | [DevOps](DEVOPS_QA_OBSERVABILITY.md) |
| Sviluppo | GitHub privato con mirror locale, Claude Code e Project Continuity Engine versionato con Git | [Sviluppo](DEVELOPMENT.md) |
| Sequenza | Contratti, prototipi, MVP avanzato, integrazione, espansione, validazione, release | [Roadmap](ROADMAP.md) |

## Scelte ancora aperte

Valutare stack mobile, motori vocali, trasporto degli eventi e RPC, implementazioni di storage e gestione segreti, soluzione VPN mesh, modalità di aggiornamento Desktop, catalogo iniziale dettagliato e valori obiettivo SLO e recovery. La struttura delle directory in DEVELOPMENT.md è una proposta operativa. I codici dei requisiti e le verifiche rappresentative sono stati aggiunti per collegare piano e implementazione.

## ADR registrati

### ADR-0001 — Primi contratti in JSON Schema e posizione dei documenti

- **Data:** 2026-11-17 · **Stato:** Accettato per la Fase 1, rivedibile dal proprietario
- **Contesto:** la Fase 1 richiede contratti iniziali verificabili (RNF-01). Il trasporto di eventi e RPC resta aperto. I 14 documenti sono stati caricati in `docs/`, mentre DEVELOPMENT.md li prevede in radice.
- **Opzioni:** JSON Schema indipendente dal trasporto; IDL legato a un RPC (Protobuf, Cap'n Proto); tipi Rust come fonte primaria.
- **Decisione:** contratti v1 in JSON Schema draft 2020-12 in `contracts/v1/`, con fixture sintetiche e test in `tests/contracts/`. I documenti restano in `docs/`. **Aggiornamento:** il proprietario ha poi spostato `CLAUDE.md` e `README.md` in radice (commit `d482b80`); i loro collegamenti puntano a `docs/`.
- **Motivazione:** JSON Schema è leggibile da Rust, Python e TypeScript e non anticipa la scelta di RPC ed event bus. Spostare i documenti cambierebbe la struttura caricata dal proprietario senza necessità tecnica.
- **Conseguenze:** la scelta RPC dovrà mappare o generare questi schemi. Gli invarianti semantici non esprimibili in schema sono elencati in `contracts/README.md` e vanno replicati nelle implementazioni.
- **Evidenza:** `python3 -m unittest discover -s tests/contracts -v` (4 test, OK).
- **Documenti interessati:** ARCHITECTURE.md, AI_SYSTEM.md, AUTONOMY_AND_TOOLS.md, DEVELOPMENT.md, `contracts/README.md`.

### ADR-0002 — Validazione dei contratti nel Core Rust e catena di hash per l'audit

- **Data:** 2026-11-17 · **Stato:** Accettato per la Fase 2, rivedibile dal proprietario
- **Contesto:** RNF-01 richiede prove di compatibilità fra moduli. TECH_STACK.md chiede di versionare dipendenze e contratti alla loro introduzione. SECURITY_AND_PRIVACY.md chiede di testare la manipolazione dei log.
- **Opzioni:** generare tipi Rust dagli schemi; validare a runtime con gli stessi schemi; mantenere tipi scritti a mano.
- **Decisione:** crate `jarvis-contracts` in `core/contracts` che incorpora gli schemi v1 a compile time e li valida con `jsonschema` 0.30, senza risoluzione HTTP o file. Gli invarianti semantici sono replicati in Rust e provati sulle stesse fixture del test Python. Gli eventi di audit formano una catena SHA-256 sul JSON canonico, con genesi di 64 zeri.
- **Motivazione:** un'unica fonte di verità (gli schemi) e fixture condivise rendono misurabile la compatibilità Rust–Python. La catena di hash rileva modifica e cancellazione di un evento senza scegliere ancora lo storage dell'audit.
- **Conseguenze:** gli invarianti semantici esistono in due implementazioni, tenute allineate dai test comuni. La catena da sola non protegge da chi riscrive tutto il flusso: firma o ancoraggio esterno restano da decidere con la gestione dei segreti. Il `Cargo.lock` è versionato.
- **Evidenza:** `cargo test --locked` in `core/` (5 test OK), `cargo clippy --all-targets -- -D warnings` (OK), `python3 -m unittest discover -s tests/contracts -v` (8 test OK).
- **Documenti interessati:** `contracts/README.md`, TECH_STACK.md, SECURITY_AND_PRIVACY.md, REQUIREMENTS.md.

### ADR-0003 — Bridge prototipale Core Rust–worker Python su stdio

- **Data:** 2026-11-17 · **Stato:** Prototipo della Fase 2; non sostituisce la scelta RPC ed event bus, che resta aperta
- **Contesto:** la Fase 2 richiede una prova riproducibile di integrazione Rust/Python. TECH_STACK.md chiede di descrivere input, output, compatibilità, propagazione degli errori, timeout e stato.
- **Opzioni:** gRPC o altro RPC con IDL; HTTP locale; JSON Lines su stdio di un processo figlio; FFI (PyO3).
- **Decisione:** prototipo in JSON Lines su stdio. Il Core (`core/bridge`, crate `jarvis-bridge`) avvia `python3 -m jarvis_ai.worker` e scambia `jarvis.envelope/1` con `delegation.request` e `delegation.result`. Entrambi i lati validano con i contratti v1. Il Core verifica che `causation_id`, `correlation_id` e `task_id` siano correlati alla richiesta, e allo scadere di `timeout_ms` termina il worker.
- **Motivazione:** nessuna dipendenza di rete o IDL anticipa la scelta RPC. L'isolamento di processo rende osservabili crash e timeout (RNF-02) e prova la compatibilità dei contratti fra linguaggi (RNF-01).
- **Conseguenze:** timeout e crash producono `effect: unknown` e `retryable: false`: prima di ripetere, l'Orchestrator deve interrogare lo stato. Il worker gestisce una richiesta alla volta. Mancano ancora multiplexing, backpressure, riavvio supervisionato e autenticazione del canale: vanno risolti con la scelta RPC o con systemd.
- **Evidenza:** `cargo test --locked` in `core/`, 6 test del bridge con il worker Python reale; `python3 -m unittest discover -s tests/ai -v`, 6 test.
- **Documenti interessati:** ARCHITECTURE.md, TECH_STACK.md, AI_SYSTEM.md, REQUIREMENTS.md.

### ADR-0004 — Prototipo integrato v0.1 testabile dal proprietario

- **Data:** 2026-11-17 · **Stato:** Accettato per la Fase 2. Il proprietario ha chiesto un prototipo funzionante da provare.
- **Contesto:** servono flussi principali dimostrabili (ROADMAP fasi 2–3) senza anticipare scelte ancora aperte: RPC, storage distribuito, segreti, VPN, voce, mobile.
- **Decisione:**
  - Binario `jarvis` (crate `jarvis-core`, axum 0.8 e tokio) con API HTTP locale e interfaccia Web servita dal Core.
  - Orchestrator sincrono, che esegue una richiesta alla volta.
  - Security Engine a regole (`policy.rs`).
  - Strumenti descritti in `contracts/catalog/tools` ed eseguiti solo dal Core, confinati in `data/workspace`.
  - Memoria, task e conversazione in SQLite (`rusqlite`, motore incluso nel binario). Audit JSONL a catena di hash.
  - AI Gateway Python con adapter Ollama e modello offline a regole, raggiunto tramite il bridge stdio (ADR-0003). Nuovo contratto `jarvis.agent-result/1` e campo facoltativo `inputs` nella delega v1.
  - Interfaccia in HTML, CSS e JavaScript senza dipendenze.
- **Controlli di sicurezza:**
  - ascolto solo su loopback;
  - controllo dell'header `Host` contro il DNS rebinding;
  - token di sessione casuale, presente solo nella pagina servita dal Core;
  - CSP restrittiva;
  - argomenti degli strumenti validati sullo schema;
  - percorsi controllati sia nella policy sia in esecuzione, compresi i link simbolici;
  - conferma obbligatoria per gli strumenti a rischio alto, monouso, legata al digest dei parametri e valida 5 minuti;
  - nessun backend cloud: `allow_cloud` è sempre falso.
- **Motivazione:** l'esperienza completa (chat, strumento, policy, conferma, esito, memoria, audit) diventa provabile end-to-end, in locale e senza rete. Il modello offline rende i test deterministici; Ollama fornisce il linguaggio naturale quando è disponibile.
- **Deviazioni temporanee da TECH_STACK.md:**
  - interfaccia senza React e senza Tauri, per evitare una toolchain di build nel prototipo;
  - SQLite usato anche per i dati centrali al posto di PostgreSQL.
  Entrambe vanno riviste nella Fase 3.
- **Conseguenze:**
  - il modello offline è un supporto di test e di funzionamento minimo, non un assistente generale;
  - la qualità del tool calling con Ollama non è stata verificata con un modello reale (Ollama non è installabile nel container: il controllo resta sospeso);
  - un solo utente, senza passkey né MFA: il token di sessione non sostituisce l'identità prevista da SECURITY_AND_PRIVACY.md.
- **Evidenza:**
  - `cargo test --locked`: 23 test, di cui 12 d'integrazione sull'API con il worker reale;
  - `python3 -m unittest discover -s tests/ai -v`: 17 test, con l'adapter Ollama provato su un server sintetico;
  - `tests/e2e/ui_smoke.cjs`: 12 verifiche in Chromium;
  - avvio da zero con `./jarvis.sh`.
- **Documenti interessati:** README.md, ARCHITECTURE.md, AI_SYSTEM.md, AUTONOMY_AND_TOOLS.md, SECURITY_AND_PRIVACY.md, MEMORY_DATA_SYNC.md, INTERFACE_AND_DEVICES.md, TECH_STACK.md, REQUIREMENTS.md, DEVOPS_QA_OBSERVABILITY.md.

### ADR-0005 — Automation Engine: workflow pianificati con approvazione legata ai passi

- **Data:** 2026-11-17 · **Stato:** Accettato per la Fase 3 (RF-07)
- **Contesto:** AUTONOMY_AND_TOOLS.md richiede workflow con versione, operazioni e frequenza, una cronologia delle esecuzioni e la policy vigente applicata anche in background. In background il proprietario non può confermare un'azione rischiosa al momento dell'esecuzione.
- **Opzioni:**
  - negare in background ogni strumento che richiede conferma;
  - chiedere la conferma a ogni esecuzione;
  - pre-approvare passi precisi alla creazione.
- **Decisione:**
  - Contratti `jarvis.workflow/1` e `jarvis.workflow-run/1`.
  - Tre tipi di trigger: `interval` (minimo 60 s), `daily` (ora locale del Core) e `once`.
  - Passi presi dal catalogo; gli strumenti `workflow.*` sono esclusi, quindi niente automazioni ricorsive.
  - Alla creazione il proprietario approva i passi esatti: con la conferma in chat, oppure con un'azione diretta dall'interfaccia autenticata. L'approvazione registra il digest SHA-256 dei passi.
  - A ogni esecuzione il Core:
    - ricalcola il digest; se non coincide l'esecuzione fallisce con `security.approval_mismatch` e l'automazione viene sospesa;
    - rivaluta la policy vigente per ogni passo: i dinieghi restano tali anche se approvati;
    - esegue i passi `require_confirmation` solo perché coperti dall'approvazione, marcandoli `preapproved`.
  - Pianificatore nel Core (tokio, ogni `JARVIS_SCHEDULER_TICK_S` secondi) con stato in SQLite. Se il Core resta spento, al riavvio recupera **una** sola esecuzione persa (`catch_up`) e poi riprende dal presente.
  - Ogni esecuzione crea un task con richiedente `module:automation`, eventi di audit e una voce di cronologia.
- **Motivazione:** il proprietario decide una volta sola su azioni precise. La policy resta l'ultima parola e ogni alterazione dei passi è rilevata. Recuperare una sola esecuzione evita raffiche di azioni dopo un lungo spegnimento.
- **Conseguenze:**
  - ~~modificare i passi richiede di creare una nuova automazione~~ superato da ADR-0008 (modifica con versioni);
  - le notifiche sugli esiti (RF-13) non ci sono ancora;
  - il fuso orario è quello del Core.
- **Evidenza:**
  - `cargo test --locked`: 10 test d'integrazione in `core/server/tests/automation.rs` e 5 test unitari del calcolo delle esecuzioni;
  - `tests/ai/test_gateway.py`: intenti di pianificazione;
  - E2E: creazione da chat e da modulo, esecuzione manuale e cronologia;
  - prova manuale del pianificatore reale su un server avviato.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, REQUIREMENTS.md, ARCHITECTURE.md, INTERFACE_AND_DEVICES.md, `contracts/README.md`.

### ADR-0006 — Notifiche e monitor delle anomalie

- **Data:** 2026-11-17 · **Stato:** Accettato per la Fase 3 (RF-13)
- **Contesto:** RF-13 chiede di monitorare attività e anomalie e di notificarle; la verifica rappresentativa è «un guasto simulato è rilevabile e correlabile al task». Le notifiche devono essere utili senza diventare rumore.
- **Decisione:**
  - Contratto `jarvis.notification/1`, conservato in SQLite (ultime 500).
  - Il Core genera notifiche per:
    - conferme in attesa: si chiudono da sole quando la conferma viene risolta;
    - conferme scadute;
    - agente AI non disponibile o degradato;
    - automazioni fallite o parziali;
    - successi delle sole automazioni singole e manuali;
    - automazioni sospese per passi alterati;
    - registro di audit alterato;
    - worker terminato all'improvviso.
  - Ogni notifica porta `correlation_id` e, quando esistono, `task_id` e `workflow_id`.
  - La `dedupe_key` impedisce duplicati finché la notifica precedente non è letta.
  - Il monitor gira a ogni intervallo del pianificatore, anche senza interfaccia aperta.
  - L'interfaccia interroga `/api/notifications?since=` ogni 5 secondi; mostra campanella con contatore, pannello e toast, e se autorizzate le notifiche del browser.
- **Opzioni scartate:**
  - SSE o WebSocket: `EventSource` non invia header personalizzati e il token di sessione finirebbe nell'URL. L'interrogazione periodica basta per un utente locale;
  - notificare ogni esecuzione periodica riuscita: troppo rumore.
- **Conseguenze:**
  - ritardo massimo di circa 5 secondi nell'interfaccia;
  - nessun canale esterno (email, mobile push), rimandato alle integrazioni e al client mobile;
  - le soglie su risorse (disco, memoria) restano da definire con gli SLO.
- **Evidenza:**
  - `core/server/tests/notifications.rs`: 7 test;
  - E2E: contatore, toast, pannello e chiusura dopo l'approvazione.
- **Documenti interessati:** DEVOPS_QA_OBSERVABILITY.md, INTERFACE_AND_DEVICES.md, REQUIREMENTS.md, ARCHITECTURE.md, `contracts/README.md`.

### ADR-0007 — Web Agent: ricerca e lettura con provenienza, SSRF e prompt injection

- **Data:** 2026-11-17 · **Stato:** Accettato per la Fase 3 (RF-06)
- **Contesto:** RF-06 chiede ricerca, navigazione e verifica delle fonti; la verifica è «una ricerca riporta fonti e provenienza delle evidenze». SECURITY_AND_PRIVACY.md chiede di trattare come dati i contenuti dei siti e di difendersi dalla prompt injection. RNF-05 chiede di minimizzare i dati in uscita. Il Core gira sul computer del proprietario, quindi un fetch arbitrario potrebbe raggiungere la rete di casa o l'API locale (SSRF).
- **Opzioni:**
  - motori di ricerca con chiave API, a pagamento e con dati verso terzi;
  - scraping di motori generalisti, fragile;
  - Wikipedia API, senza chiave e stabile;
  - SearXNG self-hosted;
  - Playwright per la navigazione completa.
- **Decisione:**
  - **Strumenti:** `web.search` (fornitori Wikipedia predefinito e SearXNG configurabile) e `web.fetch` (HTTP GET con estrazione del testo), eseguiti dal Core con `ureq`. Playwright per l'interazione con le pagine resta per una fase successiva.
  - **Policy:**
    - `destination: external`;
    - interruttore `JARVIS_WEB`;
    - domini consentiti e vietati;
    - solo http e https, nessuna credenziale nell'URL;
    - niente indirizzi privati o di loopback.
  - **Esecuzione:**
    - risoluzione DNS verificata e connessione agli indirizzi controllati;
    - controllo ripetuto a ogni redirect (al massimo 3);
    - limiti di 1 MiB, di tempo e di tipo di contenuto.
  - **Provenienza:** il Core allega le fonti a ogni risposta, indipendentemente dal testo del modello: URL, titolo, istante di recupero, SHA-256 del contenuto, fornitore. Le fonti restano salvate con il messaggio.
  - **Prompt injection:**
    - il contenuto è marcato `untrusted` e incapsulato per il modello;
    - il prompt di sistema vieta di seguirne le istruzioni;
    - strutturalmente, dopo un risultato di strumento l'Orchestrator accetta solo una risposta testuale, quindi una pagina non può avviare altre azioni nello stesso turno.
- **Motivazione:** fonti verificabili senza chiavi né costi. L'opzione self-hosted rispetta il local-first. La difesa strutturale non dipende dalla bravura del modello.
- **Conseguenze:**
  - la query di ricerca esce dal computer: il catalogo lo dichiara e l'audit registra ogni chiamata;
  - Wikipedia non è un motore generalista: per il Web intero serve SearXNG;
  - con un proxy di sistema il controllo degli indirizzi avviene sulla risoluzione locale.
- **Evidenza:**
  - `core/server/tests/web.rs`: 8 test con server locale (ricerca, estrazione, SHA-256, iniezione, SSRF, redirect verso dominio vietato, limiti, allowlist e disattivazione, audit);
  - 3 test unitari in `web.rs`;
  - `tests/ai/test_gateway.py`: intenti, citazioni, marcatura per Ollama;
  - E2E: ricerca, lettura, fonti e dominio vietato.
  - Prova sul Web reale sospesa: il proxy del container nega l'accesso a Internet.
- **Documenti interessati:** SECURITY_AND_PRIVACY.md, AUTONOMY_AND_TOOLS.md, AI_SYSTEM.md, ARCHITECTURE.md, TECH_STACK.md, REQUIREMENTS.md, README.md.

### ADR-0008 — Modifica delle automazioni con versioni e nuova approvazione

- **Data:** 2026-11-17 · **Stato:** Accettato per la Fase 3 (RF-07). Supera la limitazione «modificare i passi richiede di ricreare l'automazione» di ADR-0005.
- **Contesto:** AUTONOMY_AND_TOOLS.md chiede che un workflow conservi la versione. Modificare passi o frequenza cambia ciò che verrà eseguito in background, quindi serve una nuova decisione del proprietario. La cronologia deve continuare a dire quale versione ha prodotto ogni esecuzione.
- **Decisione:**
  - **Strumento e API:** `workflow.update` (rischio alto, con conferma) e `PUT /api/workflows/{id}` (azione diretta del proprietario, registrata nell'audit). Campi modificabili: `name`, `trigger`, `steps`, `on_failure`.
  - **Nuova versione:** ogni modifica effettiva porta `version` + 1, `updated_at` e una nuova approvazione (`chat_confirmation` o `owner_ui`) con il digest dei nuovi passi. Una modifica senza cambiamenti viene rifiutata.
  - **Storico:** la tabella `workflow_versions` conserva ogni versione approvata, anche dopo l'eliminazione. Le esecuzioni portano già `workflow_version`. `GET /api/workflows/{id}/versions` restituisce la cronologia.
  - **Pianificazione:** se cambia la frequenza, la prossima esecuzione si ricalcola da adesso; altrimenti resta invariata. Un'automazione sospesa resta sospesa.
  - **Conferma leggibile:** riporta i campi proposti; l'esito elenca le differenze (es. «frequenza: ogni ora → ogni 2 ore»).
- **Motivazione:** nessuna esecuzione in background senza un'approvazione dei passi esatti. L'identità dell'automazione, la cronologia e il legame fra esecuzione e versione restano intatti.
- **Conseguenze:**
  - il ripristino di una versione precedente non è ancora previsto: si può fare riproponendone i campi;
  - il controllo dei passi alterati (`security.approval_mismatch`) vale anche dopo ogni modifica.
- **Evidenza:**
  - `core/server/tests/automation.rs`: 6 nuovi test (conferma e versione 2, ripianificazione, rinomina con audit, modifiche non valide, automazione sospesa, manomissione dopo la modifica);
  - `tests/ai/test_gateway.py`: intenti di modifica;
  - E2E: modulo precompilato e salvataggio come v2.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, REQUIREMENTS.md, README.md, `contracts/README.md`.

### Decisione operativa — Merge automatico con CI verde

- **Data:** 2026-11-17 · **Fonte:** richiesta esplicita del proprietario in sessione («merge sempre se CI verde automaticamente»).
- **Regola:** dettagliata in [DEVELOPMENT.md](DEVELOPMENT.md#integrazione-delle-modifiche).

## Aggiornamento del registro

Per un nuovo ADR registrare identificativo, data, stato, contesto, opzioni, decisione, motivazione, conseguenze, evidenza e documenti interessati. Indicare esplicitamente quando una nuova scelta sostituisce una precedente. Se si recupera una risposta storica, collegare la fonte e verificare domanda, scelta e precisazioni prima di aggiungerla.
