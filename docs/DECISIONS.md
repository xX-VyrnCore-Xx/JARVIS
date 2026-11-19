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

I motori vocali sono stati scelti dal proprietario (ADR-0010).

Valutare stack mobile, trasporto degli eventi e RPC, implementazioni di storage, custodia della chiave dei segreti nel portachiavi del sistema (ADR-0013), soluzione VPN mesh, modalità di aggiornamento Desktop, catalogo iniziale dettagliato e valori obiettivo SLO e recovery. La struttura delle directory in DEVELOPMENT.md è una proposta operativa. I codici dei requisiti e le verifiche rappresentative sono stati aggiunti per collegare piano e implementazione.

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

### ADR-0009 — App desktop Tauri come guscio sottile del Core

- **Data:** 2026-11-17 · **Stato:** Accettato per la Fase 3 (RF-10, desktop). Supera in parte la deviazione «interfaccia senza Tauri» di ADR-0004; l'interfaccia resta in HTML, CSS e JavaScript senza React.
- **Contesto:** TECH_STACK.md prevede Tauri, React e TypeScript per il desktop Windows, macOS e Linux. L'interfaccia esiste già ed è servita dal Core con le sue difese: token, controllo dell'header Host, CSP. Duplicarla nel client creerebbe due superfici da mantenere e proteggere.
- **Opzioni:**
  - riscrivere l'interfaccia dentro Tauri con chiamate IPC al Core;
  - usare Tauri come guscio che avvia il Core e mostra la sua interfaccia;
  - impacchettare un browser.
- **Decisione:**
  - **Struttura:** app `apps/desktop/src-tauri` (Tauri 2.12, workspace Cargo separato) più crate `jarvis-launcher` (`apps/desktop/launcher`, membro del workspace del Core e quindi testato in CI).
  - **Avvio:** il launcher sceglie una porta di loopback libera e avvia `jarvis` con i dati nella cartella dell'applicazione dell'utente, l'ambiente Python di `jarvis.sh` e il log in `core.log`. Attende `/api/health`; in caso di errore mostra il motivo nella pagina di attesa.
  - **Arresto:** alla chiusura il Core viene terminato. Il Core riceve anche `JARVIS_EXIT_WITH_PARENT=stdin` e si arresta quando il processo padre chiude lo stdin, cosa che avviene anche se il padre muore in modo anomalo, su ogni sistema operativo. Senza questo, un crash dell'app lasciava Core e worker orfani (osservato nella prova con SIGKILL).
  - **Sicurezza:**
    - nessuna capability Tauri, quindi la pagina del Core non ha accesso alle API native;
    - navigazione consentita solo verso la pagina locale e il Core sulla sua porta;
    - gli altri link http(s), comprese le fonti Web, si aprono nel browser di sistema.
- **Motivazione:** una sola interfaccia, con le stesse difese e gli stessi test E2E. Il desktop aggiunge finestra, ciclo di vita e dati per utente senza logica di dominio.
- **Conseguenze:**
  - l'app richiede ancora il checkout del repository (Core compilato e ambiente Python): un installatore autonomo con Core e runtime Python inclusi, firma e aggiornamenti resta da fare (DEVOPS_QA_OBSERVABILITY.md);
  - su Linux servono le librerie WebKitGTK;
  - icona generata dal progetto.
- **Evidenza:**
  - `cargo test --locked` in `core/`: 4 test del launcher con il Core reale (avvio, interfaccia servita, arresto, uscita alla morte del padre, errore all'avvio con log, binario mancante);
  - prova manuale con Xvfb: finestra Tauri con l'interfaccia del Core (screenshot); dopo SIGKILL dell'app i processi passano da 2 a 0;
  - CI: build e clippy dell'app su Linux, macOS e Windows.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, TECH_STACK.md, DEVELOPMENT.md, ARCHITECTURE.md, REQUIREMENTS.md, README.md.

### ADR-0010 — Voce locale: faster-whisper, Piper e openWakeWord

- **Data:** 2026-11-17 · **Stato:** Accettato (RF-09). **Scelta esplicita del proprietario**: «Tutto locale», fra le opzioni locale, ibrido (sintesi del sistema operativo) e solo browser (Web Speech API, con l'audio inviato ai server del browser e quindi contrario a RNF-05). Chiude la voce «motori vocali» fra le scelte aperte.
- **Decisione:**
  - **Motori** (`ai/jarvis_ai/voice.py`, versioni fissate in `ai/requirements-voice.txt`):
    - riconoscimento: faster-whisper 1.2.1, modello `small` su CPU int8, senza download a runtime;
    - sintesi: Piper 1.8.0, voce `it_IT-paola-medium`;
    - parola di attivazione: openWakeWord 0.4.0 con il modello `hey_jarvis` incluso nel pacchetto; se manca, si riconosce «Jarvis» all'inizio della trascrizione locale;
    - motori `fake` deterministici per test e CI.
  - **Contratti:** `jarvis.voice-transcription/1` e `jarvis.voice-synthesis/1`; il motore deve essere `location: local`.
  - **API:** `POST /api/voice/transcribe` (WAV fino a 5 MiB e 60 s, `?mode=wake_word`) e `POST /api/voice/speak` (risposta `audio/wav`). Se i motori mancano rispondono 503 con il motivo, che compare anche nello stato di salute.
  - **Interfaccia:**
    - acquisizione con AudioWorklet e conversione in WAV a 16 kHz nel browser;
    - pulsante microfono con stato «in ascolto» sempre visibile;
    - risposte vocali opzionali con «Interrompi»;
    - modalità «Mani libere» opzionale: rilevamento del parlato per energia; se l'utente parla mentre JARVIS risponde, la risposta si interrompe (full-duplex);
    - la trascrizione diventa un normale messaggio di chat, con le stesse policy e conferme.
  - **Privacy e sicurezza:**
    - l'audio resta in memoria solo per la trascrizione; non viene salvato, registrato nell'audit né inviato in rete;
    - in mani libere il parlato senza parola di attivazione viene scartato;
    - le conferme delle azioni rischiose non si danno a voce (una voce estranea o registrata non deve poterle approvare);
    - la CSP ammette `media-src 'self' blob:` per riprodurre la sintesi.
- **Motivazione:** risponde a RF-09 (voce, attivazione, interruzione, stato del microfono) rispettando local-first e minimizzazione (RNF-05) e il funzionamento offline (RNF-06).
- **Conseguenze:**
  - circa 500 MB di modelli e più CPU;
  - qualità e latenza dipendono dall'hardware (le soglie andranno misurate con gli SLO);
  - i modelli non sono scaricabili nel container di sviluppo (proxy), quindi riconoscimento e sintesi reali sono da verificare sul computer del proprietario; openWakeWord reale è stato provato sul non parlato;
  - l'accesso al microfono nel WebView dell'app desktop va verificato per piattaforma.
- **Evidenza:**
  - `core/server/tests/voice.rs`: 6 test;
  - `tests/ai/test_voice.py`: 10 test, compreso openWakeWord reale (punteggi 0,0001–0,04 su silenzio, tono e rumore, con soglia 0,5);
  - E2E con microfono finto di Chromium: 8 verifiche vocali. Una di queste ha scoperto la CSP che bloccava l'audio, poi corretta.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, SECURITY_AND_PRIVACY.md, AI_SYSTEM.md, ARCHITECTURE.md, TECH_STACK.md, REQUIREMENTS.md, README.md.

### ADR-0011 — Fase 4: osservabilità, iniezione di guasti e primi SLO

- **Data:** 2026-11-17 · **Stato:** Accettato (Fase 4 «Integrazione», RNF-02, RNF-07)
- **Contesto:** la condizione di uscita della Fase 4 chiede interazioni fra moduli e gestione degli errori verificate. DEVOPS_QA_OBSERVABILITY.md chiede di correlare richiesta, task, agente, strumento e nodo, e di definire soglie con misure riproducibili.
- **Decisione:**
  - **Metriche** in memoria (`core/server/src/observability.rs`): conteggio, errori, p50, p95 e max sugli ultimi 512 campioni per endpoint (modello di rotta), strumento, delega, task e automazione. Contatori di avvii e arresti anomali del worker, deleghe fallite e notifiche. Esposte da `GET /api/metrics` (con token) e dalla sezione «Sistema» dell'interfaccia.
  - **Log strutturati** JSON Lines in `data/logs/core.jsonl`, con rotazione a 5 MiB. Eventi: `task.finished`, `delegation.failed`, `tool.failed`, `workflow.run`, `worker.spawn`, `notification`, `http.error`. Ogni riga porta `task_id`, `correlation_id` e durata, mai contenuti dell'utente.
  - **Iniezione di guasti** nel worker (`JARVIS_FAULT_INJECTION=1`, `JARVIS_FAULT=hang|crash|garbage|slow:<ms>[-once]`), attiva solo in modo esplicito e usata dai test di sistema (`core/server/tests/resilience.rs`).
  - **Avvio verificato** sui tre sistemi operativi (job CI `avvio`, `tests/smoke/start_smoke.py`).
  - **Primi obiettivi di latenza** (`tests/perf/slo.json`, misurati da `tests/perf/latency.py` nella CI) per il percorso offline.
- **Motivazione:** ogni guasto simulato deve essere rilevabile, correlato al task, isolato e seguito da un recupero automatico. Le soglie si fissano solo dopo averle misurate.
- **Conseguenze:**
  - le metriche in memoria si azzerano al riavvio; dal 2026-11-17 lo storico è persistito ed esportabile (aggiornamento sotto); restano da fare le trace distribuite;
  - gli SLO con modelli reali (Ollama, faster-whisper, Piper) vanno misurati sul computer del proprietario.
- **Evidenza:**
  - `resilience.rs`: 9 test (hang, crash e output non JSON con recupero; lentezza misurata; log correlati senza contenuti; audit cancellato rilevato; workspace rimossa; 20 richieste concorrenti; metriche protette);
  - misura locale p95: health 3,4 ms, elenco task 1,7 ms, chat 9,1 ms, chat con strumento 15,3 ms;
  - E2E: pannello Sistema.
- **Documenti interessati:** DEVOPS_QA_OBSERVABILITY.md, ARCHITECTURE.md, REQUIREMENTS.md, INTERFACE_AND_DEVICES.md, README.md.

- **Aggiornamento 2026-11-17 — persistenza ed esportazione:**
  - un'istantanea compatta (conteggi, errori, p50/p95/max per serie, contatori) è salvata in SQLite (`metrics_snapshots`) ogni 5 minuti dal pianificatore e all'arresto ordinato; conservazione di 7 giorni;
  - `GET /api/metrics/history?hours=` (fino a 168), `GET /api/metrics/export` (JSON `jarvis-metrics/1` con stato attuale e storico) e `GET /api/metrics/export?format=prometheus` (testo 0.0.4); tutte con token di sessione;
  - sezione Sistema: storico delle ultime 24 ore e pulsanti di esportazione;
  - le istantanee entrano nel backup cifrato con il database e non contengono contenuti dell'utente;
  - evidenza: `metrics_survive_restart_and_export_in_prometheus_format`, unitario `prometheus_text_format`, E2E (download Prometheus).

### ADR-0012 — Backup cifrato e ripristino verificato

- **Data:** 2026-11-17 · **Stato:** Accettato (Fase 4, RNF-08). La gestione dei segreti resta aperta: la chiave non viene conservata, deriva ogni volta dalla passphrase del proprietario.
- **Contesto:** RNF-08 chiede il ripristino su un ambiente pulito. MEMORY_DATA_SYNC.md chiede backup cifrati e la convalida di dati, metadati e permessi dopo il ripristino. SECURITY_AND_PRIVACY.md chiede la protezione dei backup.
- **Decisione:**
  - **Formato** `jarvis-backup/1` (`.jbak`):
    - intestazione `JARVISB1`, sale, nonce;
    - archivio tar cifrato con XChaCha20-Poly1305 e chiave derivata con Argon2id (64 MiB, 3 passaggi);
    - passphrase di almeno 12 caratteri.
  - **Contenuto:**
    - `manifest.json` con l'impronta SHA-256 di ogni file, lo stato della catena di audit e la sua testa;
    - istantanea coerente del database (`VACUUM INTO`, presa con il Core bloccato);
    - registro di audit;
    - workspace, esclusi i link simbolici.
  - **Ripristino:**
    - decifratura autenticata;
    - percorsi dell'archivio solo relativi;
    - confronto delle impronte;
    - catena di audit e `PRAGMA integrity_check`, entrambi verificati in una cartella di appoggio;
    - solo dopo, i dati attuali vengono spostati in `<dati>.before-restore-<istante>` e sostituiti.

    Qualsiasi errore lascia i dati intatti e rimuove la cartella di appoggio.
  - **Blocco esclusivo** `data/.lock`: un solo Core per cartella dati. All'avvio il blocco viene ritentato per 3 secondi, perché un Core appena chiuso o un processo figlio fra fork ed exec può trattenerlo per poco (causa di un test intermittente osservato il 2026-11-17). Il ripristino da riga di comando lo rifiuta se JARVIS è in esecuzione.
  - **Accessi:**
    - interfaccia: `POST /api/backup`, con token, audit e notifica; l'API non espone percorsi assoluti;
    - riga di comando: `./jarvis.sh backup` e `./jarvis.sh restore <file>`;
    - i backup si trovano in `<dati>-backups`, accanto ai dati.
- **Motivazione:** un backup senza ripristino verificato non è una garanzia. La cifratura autenticata rileva sia la passphrase sbagliata sia le alterazioni. Conservare i dati sostituiti rende reversibile anche il ripristino.
- **Conseguenze:**
  - nessuna rotazione o conservazione automatica dei backup;
  - non esiste ancora un backup pianificato;
  - limite di 1 GiB nel formato del prototipo;
  - la cancellazione di dati dal sistema non si propaga ai backup già creati (MEMORY_DATA_SYNC.md richiede di documentarlo).
- **Evidenza:**
  - `core/server/tests/backup.rs`: 5 test (ripristino completo su ambiente pulito con memoria, file, automazioni, conversazione e catena di audit estesa dopo il ripristino; passphrase errata, file alterato o non valido senza effetti sui dati; dati precedenti conservati; passphrase corta; blocco della cartella; API protetta e registrata nell'audit);
  - prova da riga di comando: backup, ripristino rifiutato con passphrase errata, ripristino rifiutato con JARVIS attivo, ripristino riuscito;
  - E2E: backup dall'interfaccia.
- **Documenti interessati:** MEMORY_DATA_SYNC.md, SECURITY_AND_PRIVACY.md, DEVOPS_QA_OBSERVABILITY.md, REQUIREMENTS.md, README.md.

### ADR-0013 — Integration Engine: connettori dichiarativi e archivio dei segreti

- **Data:** 2026-11-17 · **Stato:** Accettato (Fase 4, RF-14). Custodia della chiave nel portachiavi: ADR-0021.
- **Contesto:** RF-14 chiede che un connettore possa essere installato, autorizzato e revocato. SECURITY_AND_PRIVACY.md chiede segreti centralizzati e fuori da telemetria e checkpoint. AUTONOMY_AND_TOOLS.md chiede che ogni capacità passi da catalogo, policy e conferma.
- **Opzioni:** plugin con codice eseguibile (richiede un sandbox non ancora disponibile); connettori dichiarativi HTTP senza codice; solo strumenti statici nel catalogo.
- **Decisione:**
  - **Manifest** `jarvis.connector/1` (`contracts/v1/connector.schema.json`): `id`, `base_url`, `allowed_hosts`, autenticazione `none`, `bearer` o `header`, webhook facoltativo e operazioni `GET`/`POST` con `input_schema` chiuso. Nessun campo per i segreti: un manifest con un token viene rifiutato dallo schema.
  - **Validazione all'installazione:** l'host di `base_url` deve essere in `allowed_hosts` e superare la policy Web (niente indirizzi privati salvo `JARVIS_WEB_ALLOW_PRIVATE`). I parametri di percorso devono essere obbligatori. I nomi delle operazioni devono produrre identificatori validi.
  - **Strumenti dinamici** `connector.<id>.<operazione>` (contratto `jarvis.tool/1`, ambito `integration`). Il catalogo li accetta solo con questo prefisso, quindi non possono sostituire strumenti statici. Sono registrati solo finché il connettore è autorizzato; un connettore revocato viene negato dalla policy come strumento sconosciuto.
  - **Rischio:** le scritture verso l'esterno (`write_external`) sono sempre almeno `high`, quindi con conferma monouso, anche se il manifest dichiara meno.
  - **Delega:** il Core passa all'agente al massimo 16 strumenti dei connettori, dopo quelli statici (limite di 32 della delega), con i descrittori in `inputs.tool_descriptors` (aggiunta compatibile della v1). Il gateway non li accetta dal catalogo statico. Comando offline: «connettore <id> <operazione> chiave=valore».
  - **Esecuzione:** richiesta HTTP dal Core con DNS verificato e fissato, solo host dichiarati, nessun redirect, 1 MiB al massimo, credenziale aggiunta solo in quel momento. La risposta è marcata non attendibile come i contenuti Web.
  - **Segreti** (`secrets.rs`): tabella `secrets` cifrata per valore con XChaCha20-Poly1305; chiave casuale in `data/.secrets.key` (permessi 0600 su Unix). Le credenziali non tornano mai da API, audit, log, conversazione o agente. Il backup cifrato include la chiave, altrimenti i segreti ripristinati sarebbero illeggibili.
  - **Webhook** `POST /api/hooks/{id}`: firma `X-Jarvis-Signature: sha256=<HMAC-SHA256 del corpo>` con un segreto generato all'installazione e mostrato una sola volta. Esente dal token di sessione ma soggetto al controllo dell'host di loopback. Un evento valido diventa solo una notifica (categoria `integration`, aggiunta compatibile); non avvia strumenti né automazioni. Le firme non valide sono registrate nell'audit.
  - **Audit:** `connector.install`, `connector.authorize`, `connector.revoke`, `connector.uninstall`, con il campo facoltativo `resource` (aggiunta compatibile di `jarvis.audit-event/1`).
- **Motivazione:** i connettori dichiarativi coprono le API HTTP comuni senza eseguire codice di terzi. Restano dentro gli stessi controlli di policy, conferma, SSRF e audit degli strumenti statici. La revoca è immediata perché toglie lo strumento dal catalogo.
- **Conseguenze:**
  - nessuna paginazione, nessun metodo oltre `GET` e `POST`; OAuth aggiunto dall'ADR-0027;
  - la chiave dei segreti sta sul disco accanto ai dati: protegge da letture del database, non da chi legge l'intera cartella. Spostata nel portachiavi del sistema su macOS e Windows dall'ADR-0021;
  - i webhook arrivano solo da processi locali o da un tunnel scelto dal proprietario;
  - nessun SDK o plugin con codice.
- **Evidenza:**
  - `core/server/tests/connectors.rs`: 4 test con un servizio HTTP locale (installazione senza strumenti fino all'autorizzazione; chiamata con credenziale iniettata; credenziale assente da API, audit, conversazione e da ogni file della cartella dati; ripresa dopo il riavvio; conferma delle scritture; revoca immediata anche su una conferma già emessa; webhook valido, alterato, senza firma o sconosciuto; manifest non validi);
  - unitari in `connectors.rs` (3) e `secrets.rs` (1);
  - fixture del contratto (1 valida, 5 non valide);
  - E2E: installazione dall'interfaccia, autorizzazione e chiamata dalla chat.
- **Documenti interessati:** REQUIREMENTS.md, SECURITY_AND_PRIVACY.md, AUTONOMY_AND_TOOLS.md, README.md.
- **Aggiornamento 2026-11-17 — guasti dei connettori:**
  - errori distinti: `connector.server_error` (5xx), `connector.http_error` (4xx), `web.timeout`, `web.unreachable`;
  - le letture (`retry: safe`, 2 tentativi) sono ripetute una volta sugli errori transitori; le scritture mai, perché potrebbero essere già arrivate al servizio;
  - una scrittura interrotta da timeout o rete è registrata con `effect: unknown` nell'audit e nel passo, e la notifica chiede di verificare prima di ripeterla;
  - ogni fallimento di connettore, da chat o da automazione, produce una notifica `integration` correlata al task e deduplicata per connettore;
  - evidenza: `connector_faults_are_retried_safely_and_notified`.
- **Custodia della chiave:** risolta dall'ADR-0021 (portachiavi di macOS e Windows; file 0600 su Linux).

### ADR-0014 — Dispositivi associati e app Web installabile (Fase 5, RF-11 e RF-10 mobile)

- **Data:** 2026-11-17 · **Stato:** Accettato. Restano aperte la VPN mesh (decisione 81 e seguenti), l'identità crittografica dei nodi con mTLS e le app native.
- **Contesto:** RF-11 chiede che un dispositivo revocato perda accesso e assegnazioni. RF-10 prevede il client mobile. Il Core ascolta solo su loopback e la pagina locale riceve il token di sessione: esporla in rete senza altri controlli darebbe il token a chiunque la apra.
- **Opzioni:** ascolto su LAN con TLS autofirmato (avvisi del browser, niente service worker); app native (fuori portata della milestone); Core sempre su loopback, pubblicato da un proxy fidato con HTTPS valido, più associazione dei dispositivi.
- **Decisione:**
  - **Accesso remoto solo tramite proxy fidato** verso `127.0.0.1` (per esempio `tailscale serve`, che fornisce un certificato valido nella rete privata del proprietario). Il proprietario ammette esplicitamente i nomi con `JARVIS_ALLOWED_HOSTS`; nomi senza punto o con caratteri jolly vengono ignorati. Ogni altro host resta rifiutato.
  - **Pagina remota senza token di sessione;** il token di sessione non vale da un host remoto.
  - **Associazione:**
    - codice monouso di 8 caratteri (alfabeto senza caratteri ambigui, 40 bit), valido 10 minuti, generato solo dal computer del Core;
    - dopo 5 codici errati tutti i codici in attesa vengono annullati, con notifica di sicurezza;
    - il dispositivo riceve una sola volta un token di 256 bit; il Core ne conserva solo l'impronta SHA-256 (contratto `jarvis.device/1`, senza token).
  - **Permessi:** `chat` (chat, attività, conferme in attesa, notifiche, voce, stato) e, se il proprietario lo concede, `confirm` (approvare le azioni rischiose). Dispositivi, connettori, backup, audit, memoria, workspace, automazioni e metriche restano al computer del proprietario.
  - **Revoca immediata:** l'impronta viene cancellata e la richiesta successiva riceve 401. L'interfaccia del dispositivo torna alla pagina di associazione. Associazione e revoca sono registrate nell'audit.
  - **App Web installabile:** manifest, icona e un service worker che non memorizza dati (senza rete mostra solo un avviso). La CSP ammette `manifest-src` e `worker-src 'self'`.
- **Motivazione:** il Core non si espone mai direttamente in rete. Il canale cifrato lo fornisce uno strumento già maturo scelto dal proprietario. Il controllo d'accesso e la revoca restano nel Core, con permessi minimi per i dispositivi.
- **Conseguenze:**
  - le richieste dei dispositivi sono attribuite al proprietario nei task (l'audit registra associazione e revoca, non ogni messaggio);
  - nessun nodo esecutore distribuito: i dispositivi sono client, non eseguono strumenti;
  - va verificato sul computer del proprietario che il proxy scelto conservi l'header `Host`.
- **Evidenza:**
  - `core/server/tests/devices.rs`: 4 test (pagina remota senza token, host sconosciuti rifiutati, token di sessione non valido da remoto, rotte del proprietario negate, conferma solo con il permesso `confirm`, revoca immediata nell'audit, codice monouso, blocco dopo 5 errori);
  - fixture `device.*`;
  - E2E con un secondo contesto del browser su `jarvis.test`: rimando all'associazione, sezioni del proprietario nascoste, chat dal dispositivo, revoca con perdita immediata dell'accesso.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, SECURITY_AND_PRIVACY.md, REQUIREMENTS.md, README.md.

### ADR-0015 — Release precompilata, rollback di release e audit delle dipendenze (Fasi 6 e 7)

- **Data:** 2026-11-17 · **Stato:** Accettato. Restano decisioni del proprietario: firma dei binari e degli installatori, canali Stable/Beta. Prima release: `v0.1.0`, richiesta dal proprietario e pubblicata subito, prima delle prove P1–P12 (prototipo, non 1.0.0). Il 2026-11-18 il proprietario sceglie `v1.0.0` per la release finale con la configurazione guidata. La CI installa e prova l'app sui 3 sistemi; le prove manuali P1–P13 restano da fare e gli eventuali difetti andranno in una 1.0.x.
- **Contesto:**
  - RNF-08 chiede il rollback di release;
  - DEVOPS_QA_OBSERVABILITY.md chiede release riproducibili e verifica crittografica;
  - finora l'avvio richiedeva Rust e un compilatore C sul computer del proprietario.
- **Decisione:**
  - **Archivio di release** (`scripts/package.py`): Core precompilato in `bin/`, runtime AI, contratti, lanciatori, README e [ACCEPTANCE.md](ACCEPTANCE.md); file `.sha256` accanto all'archivio. I lanciatori riconoscono la release (presenza di `bin/jarvis` e assenza di `core/`) e saltano la compilazione: serve solo Python. L'app desktop resta fuori dall'archivio.
  - **Workflow `release.yml`:** costruisce gli archivi su Linux, macOS e Windows, li verifica, li estrae senza sorgenti del Core e li avvia con i lanciatori (prova di avvio completa). Con un tag `v*` pubblica la release (aggiornamento del 2026-11-17: il proprietario ha chiesto la pubblicazione diretta, prima prevista come bozza); i tag con suffisso diventano prerelease. Il tag si crea solo su richiesta del proprietario, con un push del tag o con l'avvio manuale del workflow (input `tag`). In quel caso il workflow crea il tag sul commit costruito e rifiuta un tag che non corrisponde alla versione del Core. Questa via esiste perché le sessioni remote possono pubblicare solo il proprio branch.
  - **Rollback di release** (`rollback.rs`):
    - la cartella dati registra in `VERSION` l'ultima versione che l'ha usata;
    - quando parte una versione diversa, prima di aprire il database (quindi prima di qualunque migrazione) il Core copia database, audit, chiave dei segreti e workspace (fino a 1 GiB) in `<dati>-rollback/<versione>-<istante>/` e conserva le ultime 3 copie;
    - una notifica indica come tornare indietro;
    - `jarvis rollback` (anche `./jarvis.sh rollback`) ripristina l'ultima copia con JARVIS chiuso e sposta i dati sostituiti in `<dati>.before-rollback-<istante>`.
  - **Audit delle dipendenze in CI:** `cargo audit` per Core e app desktop, `pip-audit` per le dipendenze Python. Una vulnerabilità nota fa fallire la CI; gli avvisi «unmaintained» restano visibili ma non bloccano.
- **Prima evidenza dell'audit:** RUSTSEC-2026-0459 (CVE nella SQLite inclusa in `libsqlite3-sys` 0.30.1, gravità media 6,8), corretta aggiornando `rusqlite` da 0.32 a 0.43 (`libsqlite3-sys` 0.40), senza modifiche al codice. Avvisi aperti non bloccanti:
  - `chrono` senza manutentore, senza alternativa indicata;
  - dipendenze transitive di Tauri e GTK nell'app desktop.
- **Motivazione:** il proprietario può provare JARVIS senza toolchain. Il rollback non dipende da una passphrase e precede sempre le migrazioni. Le copie hanno la stessa protezione della cartella dati che duplicano.
- **Conseguenze:**
  - archivi non firmati: l'integrità si verifica con l'impronta SHA-256, non l'autenticità dell'autore;
  - su macOS Gatekeeper può chiedere conferma per un binario non firmato;
  - le copie per il rollback occupano spazio accanto ai dati.
- **Evidenza:**
  - `core/server/tests/rollback.rs`: 2 test (copia al cambio di versione e non a parità di versione, notifica, ripristino, catena di audit valida, solo le ultime 3 copie);
  - prova manuale: archivio Linux estratto e avviato con `jarvis.sh` senza cargo nel PATH, `start_smoke.py` superato;
  - `cargo audit` e `pip-audit` puliti dopo l'aggiornamento.
- **Documenti interessati:** ACCEPTANCE.md, DEVOPS_QA_OBSERVABILITY.md, REQUIREMENTS.md, SECURITY_AND_PRIVACY.md, README.md.

### ADR-0016 — Programmi di installazione al posto degli archivi

- **Data:** 2026-11-18 · **Stato:** Accettato su richiesta del proprietario («pubblica direttamente i file di setup installabili e non i file zip»). Sostituisce, per la pubblicazione, gli archivi dell'ADR-0015.
- **Contesto:** gli archivi richiedevano Python installato e l'avvio da terminale. Il proprietario vuole file che si installano come qualunque applicazione.
- **Decisione:**
  - **Bundler di Tauri sull'app desktop**, con la configurazione aggiuntiva `apps/desktop/src-tauri/tauri.bundle.conf.json`, usata solo per i pacchetti: la build di sviluppo e la CI desktop non ne dipendono. Formati: Windows `.msi` e setup NSIS `.exe` (per l'utente corrente), macOS `.dmg` (Apple Silicon), Linux `.deb` e `.AppImage` (costruiti su Ubuntu 22.04 per compatibilità con glibc più vecchie).
  - **Contenuto:**
    - il Core come sidecar `jarvis-core`, chiamato così per non coincidere con l'eseguibile `JARVIS` sui file system che ignorano maiuscole e minuscole;
    - `ai/jarvis_ai` e `contracts/` tra le risorse;
    - un **Python autonomo** (python-build-standalone, CPython 3.12) con le dipendenze del runtime AI già installate. `scripts/fetch_python.py` lo sceglie dall'ultima release e ne verifica l'impronta SHA-256 pubblicata.
  - **Avvio dell'app installata:** se le risorse contengono il runtime AI, l'app usa il Core accanto al proprio eseguibile e il Python incluso, con `PYTHONDONTWRITEBYTECODE=1` (le risorse possono essere in sola lettura). Dai sorgenti resta il comportamento precedente. I dati vanno nella cartella dell'applicazione dell'utente.
  - **Prove nella CI di release:**
    - Linux: installazione reale del `.deb` con `dpkg`;
    - Windows: installazione reale dell'`.msi` con `msiexec /qn`;
    - macOS: l'app viene copiata dal `.dmg` montato, come in un'installazione;
    - in tutti i casi, il Core installato con il Python incluso supera `tests/smoke/start_smoke.py`. Si pubblicano solo gli installer, con l'elenco firmato `SHA256SUMS` (ADR-0020).
- **Motivazione:** si installa e si avvia senza prerequisiti né terminale. Le prove usano i file installati, non la cartella di build.
- **Conseguenze:**
  - installer non firmati (avvisi di SmartScreen e Gatekeeper finché il proprietario non fornisce i certificati);
  - circa 50 MB per installer per via del Python incluso;
  - macOS solo per Apple Silicon;
  - i motori vocali reali restano da installare a parte (`voice-setup` vale per i sorgenti).
- **Evidenza (2026-11-18, container Linux):**
  - `.deb` costruito con il Core release e un Python sostitutivo (il download di python-build-standalone è bloccato dal proxy della sessione);
  - installato con `dpkg -i`: Core in `/usr/bin/jarvis-core`, risorse in `/usr/lib/JARVIS`;
  - Core installato più Python incluso: `start_smoke.py` superato;
  - app `jarvis-desktop` avviata sotto Xvfb: avvia il Core incluso con il Python incluso e il worker AI; alla chiusura non resta alcun processo;
  - pacchetto rimosso con `dpkg -r`.
- **Documenti interessati:** README.md, ACCEPTANCE.md, INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0017 — Nodi esecutori (Fase 5, RF-11)

- **Data:** 2026-11-18 · **Stato:** Accettato. Restano aperte l'identità crittografica con mTLS (sostituirà il token del nodo), la VPN mesh e le capacità oltre file e sistema. La scelta automatica è disponibile con ADR-0031.
- **Contesto:** il piano prevede nodi autorizzati che eseguono il lavoro assegnato dal Core. RF-11 chiede che un dispositivo revocato perda accesso e assegnazioni.
- **Opzioni:**
  - il Core si collega al nodo (porte aperte sul nodo, firewall e NAT da gestire);
  - il nodo chiede lavoro al Core con richieste in uscita, attraverso lo stesso proxy fidato dei dispositivi;
  - una coda esterna (servizio in più da gestire).
- **Decisione:**
  - **Associazione:** il proprietario genera un codice con il solo permesso `node`. Sull'altro computer si esegue `./jarvis.sh node <indirizzo-del-Core> <codice>` (stesso binario del Core, comando `jarvis node`). Il nodo dichiara gli strumenti che offre fra `system.info`, `time.now`, `fs.list`, `fs.read` e `fs.write`, e conserva il token in `node.json` con permessi 0600. Il documento `jarvis.device/1` aggiunge `slug` e `capabilities` (aggiunta compatibile).
  - **Strumenti** `node.<slug>.<strumento>` nel catalogo dinamico: stessi schema, rischio e conferma dello strumento locale, ambito `node`. Policy, conferme e audit sono quelli del Core; i percorsi fuori dalla workspace sono negati dal Core prima della consegna e di nuovo dal nodo in esecuzione. L'effetto atteso nella conferma indica il nodo.
  - **Trasporto:** richiesta lunga `GET /api/nodes/next` (fino a 25 s) ed esito su `POST /api/nodes/result/{id}`. Il nodo non apre porte.
  - **Hub delle assegnazioni** (`nodes.rs`) con blocco proprio e impronte dei token. Il Core attende l'esito tenendo il proprio blocco: le richieste dei nodi non possono passare da quel blocco senza bloccarsi a vicenda. L'hub accetta solo esiti di assegnazioni consegnate a quel nodo.
  - **Guasti:**
    - nodo non in linea (nessuna richiesta nell'ultimo minuto): `node.offline` immediato;
    - errori di trasporto e risposte HTTP 408, 429 o 5xx durante la consegna: fino a tre invii dello stesso esito, senza riesecuzione dello strumento. Il Core registra il primo esito ricevuto per un'assegnazione consegnata al nodo;
    - nessuna risposta entro `JARVIS_NODE_TIMEOUT_MS` (30 s): `node.timeout`, assegnazione ritirata, effetto `unknown` per le scritture;
    - uno strumento non dichiarato viene rifiutato anche dal nodo.
  - **Revoca:** l'API registra la revoca e ritira il token nella stessa transazione SQLite. L'hub svuota la coda e risveglia chi attende un esito con `node.revoked` prima che l'API acquisisca il lock del Core per rimuovere gli strumenti e registrare l'audit. Una scrittura interrotta dalla revoca ha effetto `unknown`; l'agente si ferma quando il Core rifiuta il token.
  - **Comando offline:** «nodo <slug> <strumento> chiave=valore». Con Ollama gli strumenti del nodo arrivano all'agente come descrittori dinamici.
- **Motivazione:** il nodo funziona anche dietro NAT senza aprire porte e senza servizi aggiuntivi, e riusa associazione, revoca e permessi minimi già verificati per i dispositivi.
- **Conseguenze:**
  - il Core serve una richiesta alla volta mentre attende un nodo (limite già noto del prototipo);
  - le scritture interrotte da un timeout hanno esito da verificare;
  - il nodo non ha memoria, Web né automazioni propri.
- **Evidenza:**
  - `core/server/tests/nodes.rs`: 3 test con Core HTTP reale e agente in un altro thread (strumenti nel catalogo, lettura eseguita dal nodo, scrittura con conferma nella workspace del nodo e non in quella del Core, uscita dalla workspace negata, audit, permessi 0600 del token, revoca con agente fermato e strumenti rimossi, token revocato rifiutato, codice per chat che non associa un nodo, token del nodo che non apre l'API del proprietario, nodo non in linea);
  - unitari in `nodes.rs` (2): attesa, ritiro, esito da un altro nodo rifiutato, revoca durante l'attesa;
  - `agent_retries_delivery_of_the_same_result_after_a_transient_failure`: al primo invio il Core HTTP risponde 503, al secondo accetta l'esito per la stessa assegnazione e la chat riceve il risultato;
  - `api_revocation_interrupts_an_assignment_waiting_for_a_result`: revoca HTTP durante una scrittura assegnata, esito `node.revoked`, effetto `unknown` e token ritirato anche in SQLite;
  - prova manuale: `./jarvis.sh node` come secondo processo, «nodo laboratorio system.info» dalla chat, revoca con uscita del processo («il nodo è stato revocato»);
  - E2E: codice per nodo con il comando da eseguire.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, SECURITY_AND_PRIVACY.md, REQUIREMENTS.md, ACCEPTANCE.md, README.md.

### ADR-0018 — Client Android nativo (RF-10)

- **Data:** 2026-11-18 · **Stato:** Accettato. Restano da fare la prova su dispositivo (P8b), la firma per la distribuzione, le notifiche push e la voce nativa.
- **Contesto:** RF-10 prevede client mobili nativi. Il client iOS (`apps/ios`) copre associazione, chat e conferme; Android aveva solo la PWA.
- **Decisione:**
  - **Due moduli Gradle in `apps/android`:**
    - `core-client`, in Kotlin puro: tutta la logica di rete, compilabile e verificabile senza Android SDK;
    - `app`, in Jetpack Compose (minSdk 26, targetSdk 35).
  - **Stesse regole del client iOS:**
    - indirizzo solo HTTPS, senza credenziali, query o percorsi;
    - nessun redirect e nessuna cache;
    - token di 64 caratteri esadecimali, inviato solo nell'header `X-Jarvis-Device`;
    - associazione rifiutata senza il permesso `chat`, quindi un codice per nodi non associa il telefono;
    - le conferme partono solo con il permesso `confirm`;
    - 401 vale come revoca e riporta all'associazione.
  - **Sessione cifrata** con AES-GCM e una chiave dell'Android Keystore, che non lascia il telefono. Backup dell'app disattivato e traffico in chiaro vietato nel manifest.
  - **CI:** job «Android» con i test di `core-client` e l'APK di debug come artefatto. In locale, senza SDK: `gradle -p apps/android -Pjarvis.coreOnly=true :core-client:test`.
- **Motivazione:** la logica di rete resta verificabile ovunque e allineata al client iOS; l'interfaccia è un livello sottile.
- **Conseguenze:**
  - l'APK di debug non è firmato per la distribuzione;
  - la voce e le notifiche push restano alla PWA;
  - l'app non è verificata su un dispositivo reale fino alla prova P8b.
- **Evidenza:** `CoreClientTest` (5 test: indirizzi ammessi e rifiutati, associazione con piattaforma `android` e token valido, codice per nodi rifiutato, token del dispositivo su ogni richiesta, conferma senza permesso bloccata prima della rete, identificativo in minuscolo, revoca, messaggi d'errore del Core); build dell'APK nella CI.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ACCEPTANCE.md, REQUIREMENTS.md, README.md.

### ADR-0019 — Configurazione guidata dei modelli al primo avvio e rifinitura per macOS e Windows

- **Data:** 2026-11-18 · **Stato:** Accettato. Richiesta del proprietario: «durante l'installazione seleziona già i modelli da scaricare e tutto il resto».
- **Contesto:**
  - un `.dmg` di macOS non può eseguire codice durante l'installazione;
  - l'app installata è in sola lettura;
  - la voce richiedeva `voice-setup` da terminale, disponibile solo con i sorgenti;
  - il modello di Ollama andava scaricato a mano.
- **Decisione:**
  - **Configurazione guidata al primo avvio**, uguale su macOS, Windows e Linux. La finestra si apre da sola con le scelte consigliate già selezionate e si riapre da Sistema → «Configura voce e modello AI»:
    - voce: faster-whisper `small`, Piper `it_IT-paola-medium`, openWakeWord «hey_jarvis»;
    - modello AI: Ollama `qwen2.5:3b`, preselezionato solo se Ollama è attivo; altrimenti la finestra porta a ollama.com/download.
    
    La finestra mostra la dimensione stimata e l'avanzamento di ogni passo.
  - **Backend `setup.rs`:**
    - passi eseguiti in un thread separato, fuori dal blocco del Core;
    - pacchetti della voce installati con `pip --target` in `<dati>/python-packages` usando il Python dell'app; modelli in `<dati>/models`;
    - modello di Ollama scaricato con la sua API `/api/pull`, con avanzamento in byte;
    - valori accettati solo dagli elenchi noti, quindi nessun argomento arbitrario a pip o a Ollama;
    - una sola configurazione alla volta;
    - le scelte vengono salvate in `<dati>/setup.json`;
    - al termine il worker AI riparte con `PYTHONPATH`, `JARVIS_MODELS_DIR`, `JARVIS_TTS_VOICE`, `JARVIS_STT_MODEL` e `JARVIS_OLLAMA_MODEL`, senza sovrascrivere le variabili del proprietario.
  - **Nell'app desktop i collegamenti con `target=_blank`** (per esempio il download di Ollama) si aprono nel browser di sistema, mai in una nuova finestra dell'app.
  - **Windows:** l'app desktop avvia il Core, e il Core il worker Python e pip, con `CREATE_NO_WINDOW`: niente finestre di console accanto all'app.
  - **Versione nell'intestazione** letta dal Core, non più fissa.
  - **CI di release:** su macOS, Windows e Linux i pacchetti della voce si installano con il Python incluso e si importano (`faster_whisper`, `piper`, `openwakeword`, `onnxruntime`), come farà la configurazione guidata.
- **Motivazione:** chi installa l'app ottiene voce e modello con due clic, senza terminale e senza scrivere nell'app installata.
- **Conseguenze:**
  - il primo avvio scarica circa 2,7 GB con le scelte consigliate;
  - Ollama si installa a parte (ha un proprio installer firmato);
  - la configurazione richiede rete.
- **Evidenza:**
  - `core/server/tests/setup.rs` (2: passi, rifiuto di valori non ammessi, configurazione unica, scelte salvate, worker funzionante dopo il riavvio; ambiente del worker) e unitario `plan_accepts_only_known_values`;
  - E2E con passi simulati: finestra al primo avvio con le scelte consigliate, dimensione stimata, completamento dei passi;
  - installazione della voce con il Python incluso verificata dalla CI di release sui 3 sistemi.
- **Documenti interessati:** README.md, ACCEPTANCE.md, INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0020 — Firma delle release senza chiavi e una sola release pubblicata

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («firma autonomamente le release», «lascia solamente quelle necessarie»).
- **Contesto:** gli installer non erano firmati. Un certificato Apple Developer ID o Authenticode richiede un acquisto e un'identità del proprietario, che la sessione non può ottenere. Erano pubblicate tre release (`v0.1.0`, `v0.1.1`, `v1.0.0`).
- **Opzioni:**
  - chiave GPG o minisign custodita nei segreti del repository: richiede di generarla e conservarla fuori dalla sessione;
  - firma senza chiavi con l'identità OIDC del workflow (Sigstore e attestazioni di GitHub): nessun segreto, verificabile da chiunque;
  - certificati commerciali: rinviati al proprietario.
- **Decisione:**
  - il job di pubblicazione crea un'attestazione di provenienza (`actions/attest-build-provenance`) per ogni installer;
  - crea un unico `SHA256SUMS` al posto dei file `.sha256` separati e lo firma con `cosign sign-blob` senza chiavi. Il bundle `SHA256SUMS.sigstore.json` viene verificato nel job stesso e pubblicato con la release;
  - l'app macOS ha una firma ad hoc (`signingIdentity: "-"`), verificata in CI con `codesign --verify`;
  - dopo una release stabile, il workflow elimina le altre release e i loro tag. Una prerelease non elimina nulla;
  - l'input `sostituisci` ricrea una release esistente con lo stesso tag sul commit costruito.
- **Conseguenze:**
  - l'origine di ogni file è verificabile con `gh attestation verify` oppure con `cosign verify-blob` e `sha256sum -c`;
  - SmartScreen e Gatekeeper mostrano ancora un avviso al primo avvio finché non si usano certificati commerciali;
  - il ritorno a una versione precedente resta locale (`jarvis rollback`, ADR-0015).
- **Evidenza:** workflow «release»: verifica `cosign verify-blob` nel job di pubblicazione, `codesign --verify` dell'app installata dal `.dmg`, elenco finale delle release.
- **Documenti interessati:** README.md, ACCEPTANCE.md, ROADMAP.md.

### ADR-0021 — Chiave dei segreti nel portachiavi del sistema

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** ADR-0013 (decisione aperta), prossima attività del checkpoint.
- **Contesto:** la chiave che cifra i segreti dei connettori stava in `data/.secrets.key`, accanto al database: chi leggeva l'intera cartella dei dati poteva decifrarli.
- **Opzioni:**
  - `keyring` con i portachiavi nativi di macOS e Windows; su Linux Secret Service via D-Bus;
  - solo macOS e Windows, Linux con il file 0600;
  - chiave derivata da una passphrase chiesta all'avvio.
- **Decisione:**
  - su macOS (portachiavi di login) e Windows (Gestione credenziali) la chiave sta nel portachiavi, nel servizio `JARVIS`, con una voce per cartella dei dati (`secrets-key-` più un'impronta del percorso);
  - su Linux resta il file 0600: Secret Service richiede D-Bus e una sessione grafica, assenti su server e runner;
  - un file di chiave trovato all'apertura (versione precedente o ripristino di un backup) viene copiato nel portachiavi, riletto e solo allora cancellato;
  - se il portachiavi non risponde o la voce manca mentre esistono segreti, il Core non crea una chiave nuova, che li renderebbe illeggibili: l'apertura fallisce con un messaggio esplicito. Senza segreti ripiega sul file;
  - il backup cifrato include la chiave letta dal file o dal portachiavi; `JARVIS_SECRETS_KEYCHAIN=off` forza il file;
  - `/api/health` riporta `core.secrets_store` (`keychain` o `file`).
- **Conseguenze:**
  - copiare la cartella dei dati non basta più per leggere i segreti su macOS e Windows;
  - con la firma ad hoc (ADR-0020), dopo un aggiornamento macOS può chiedere di consentire di nuovo l'accesso alla voce del portachiavi;
  - Linux resta protetto solo dai permessi del file.
- **Evidenza:**
  - unitari in `secrets.rs`: spostamento dal file con segreti leggibili, nuova installazione, portachiavi bloccato o voce mancante senza chiave nuova, ripiego sul file, voce distinta per cartella;
  - CI «avvio reale»: `core.secrets_store` vale `keychain` su macOS e Windows, `file` su Linux.
- **Documenti interessati:** SECURITY_AND_PRIVACY.md, ACCEPTANCE.md, ROADMAP.md.

### ADR-0022 — Memoria semantica, procedurale ed episodica con indici locali ricostruibili

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («finisci tutte le fasi e la memoria»), MEMORY_DATA_SYNC.md.
- **Contesto:** la memoria era un elenco di frasi. L'agente riceveva i primi 50 elementi, senza tipi, provenienza, ricerca o correzione. La specifica chiede memoria episodica, semantica e procedurale, ricerca semantica, knowledge graph e indici derivati ricostruibili.
- **Opzioni:**
  - PostgreSQL con pgvector o Qdrant: servizi esterni, non adatti a un'installazione desktop autonoma;
  - embeddings da Ollama: qualità migliore, ma assenti senza Ollama e diversi a ogni cambio di modello, quindi l'indice andrebbe ricostruito;
  - SQLite con FTS5 e un vettore locale con hashing: sempre disponibile, deterministico e ricostruibile.
- **Decisione:**
  - colonne additive su `memory` (`kind`, `source`, `updated_at`, `expires_at`) con migrazione automatica dei database esistenti;
  - indici `memory_fts`, `memory_vectors` e `memory_edges` derivati dalla tabella e ricostruiti all'avvio se mancano;
  - ricerca ibrida con soglia; contesto dell'agente con preferenze, fatti ed episodi pertinenti;
  - episodi automatici per le attività della chat, con scadenza e senza le operazioni sulla memoria;
  - API `GET /api/memory?kind=&q=`, `PATCH /api/memory/{id}`, `GET /api/memory/graph`, `POST /api/memory/reindex`, con audit per correzione e ricostruzione;
  - strumento `memory.search` e parametro `kind` per `memory.remember` e `memory.list`.
- **Conseguenze:**
  - la ricerca riconosce flessioni e accenti, non i sinonimi: «auto» non trova «macchina». Embeddings da modello restano un'estensione possibile, perché l'indice è derivato;
  - l'estrazione delle triple copre le frasi italiane comuni («il mio X si chiama Y», «abito a …», «X è Y»), non frasi arbitrarie;
  - un episodio contiene la richiesta originale (160 caratteri): si cancella come ogni altro elemento e scade da solo.
- **Evidenza:**
  - `core/server/tests/memory.rs`: tipi, ricerca tollerante alle flessioni, grafo, episodi, correzione e cancellazione propagate, ricostruzione, audit;
  - unitari in `memory.rs`: vettori, triple, tipo dedotto, migrazione di un database precedente;
  - `tests/ai/test_gateway.py`: preferenze e ricerca dalla chat offline;
  - E2E: filtro per tipo, ricerca, grafo nella sezione Memoria.
- **Documenti interessati:** MEMORY_DATA_SYNC.md, README.md, ROADMAP.md, ACCEPTANCE.md.

### ADR-0023 — Dashboard React e TypeScript con tema HUD olografico

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («crea la dashboard completa moderna e futuristica»); scelte confermate: React + TypeScript, stile HUD olografico. Sostituisce la deviazione temporanea dall'uso di React dell'ADR-0004.
- **Contesto:** TECH_STACK.md prevede React e TypeScript per la dashboard Web, ma il prototipo usava JavaScript senza dipendenze. INTERFACE_AND_DEVICES.md chiede una dashboard con attività, esecutore, condizioni offline, errori e conferme in attesa.
- **Opzioni:**
  - evolvere l'interfaccia esistente: rapido, ma lontano dallo stack documentato;
  - React e TypeScript con un server di sviluppo separato: complica avvio e CSP;
  - React e TypeScript compilati in un bundle fisso servito dal Core.
- **Decisione:**
  - `apps/web` (Vite 8, React 19.3, TypeScript 5.9 in modalità strict) produce `core/server/static/app.js` (IIFE) e `app.css`, inclusi nel Core con `include_str!`. Restano invariati la CSP (`script-src 'self'`), `boot.js` e l'iniezione del token;
  - il bundle è versionato: Rust, installer e `jarvis.sh` non richiedono Node. La CI lo ricostruisce, verifica che sia identico e controlla con `npm audit` le dipendenze distribuite;
  - tutte le funzioni precedenti sono state portate con gli stessi ID e classi: chat, conferme, voce con mani libere e interruzione, avatar, notifiche, attività, automazioni, memoria, workspace, connettori, dispositivi, audit, sistema, backup e configurazione guidata;
  - nuova vista iniziale «Plancia» per il proprietario; tema scuro HUD con variante chiara; layout adattato fino a 390 px; movimento ridotto rispettato.
- **Conseguenze:**
  - modificare l'interfaccia richiede `npm run build` in `apps/web` prima del commit;
  - il bundle pesa circa 290 kB (90 kB compressi), servito localmente;
  - il motore vocale è un modulo TypeScript testato con `tests/ui/audio_offline.test.cjs`.
- **Evidenza:**
  - E2E `tests/e2e/ui_smoke.cjs`: 66 verifiche, comprese plancia, comando rapido, conferme in attesa ed esecutore;
  - controllo del bundle nel job e2e; `tsc --noEmit` nel build.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, TECH_STACK.md, DEVELOPMENT.md, CLAUDE.md, ROADMAP.md.

### ADR-0024 — Budget dei token verso il modello locale

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («miglioramento di uso dei token»).
- **Contesto:**
  - ogni delega inviava fino a 10 turni da 8000 caratteri, fino a 50 elementi di memoria e la descrizione completa di tutti gli strumenti (16 più connettori e nodi);
  - con modelli locali da 3–7 miliardi di parametri questo allunga la valutazione del prompt, può superare la finestra di contesto e peggiora la scelta dello strumento.
- **Opzioni:**
  - riassumere la conversazione con il modello: costa una chiamata in più;
  - budget deterministici nel Core e selezione degli strumenti nel worker: nessuna chiamata in più, comportamento verificabile.
- **Decisione:**
  - cronologia e memoria entro budget nel Core (`budget_history`, `budget_memory`);
  - selezione degli strumenti per parole chiave con un nucleo fisso;
  - schemi compatti e JSON compatto per gli esiti;
  - `num_ctx` e `keep_alive` espliciti;
  - campo facoltativo `usage` in `jarvis.agent-result/1` (aggiunta compatibile) con contatori nel Core.
- **Conseguenze:**
  - se la richiesta non contiene parole chiave note, il modello vede solo il nucleo: per i casi ambigui resta `JARVIS_TOOL_SELECTION=all`;
  - i turni vecchi accorciati perdono dettagli, che restano nella conversazione salvata e nella memoria episodica.
- **Evidenza:**
  - `budget_tests` in `orchestrator.rs` (ultimo turno intero, budget rispettato, memoria in ordine di priorità);
  - `test_token_budget_selects_relevant_tools_and_reports_usage` in `tests/ai/test_gateway.py`.
- **Documenti interessati:** AI_SYSTEM.md, ROADMAP.md, contratto `agent-result`.

### ADR-0025 — Streaming delle risposte dal modello alla dashboard

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («procedi con lo streaming delle risposte»).
- **Contesto:** con Ollama la risposta compariva solo a generazione finita, anche dopo molti secondi.
- **Opzioni:**
  - WebSocket: un canale in più da proteggere (host, token, CSP);
  - Server-Sent Events: solo GET, mentre il messaggio va in POST;
  - `fetch` con corpo NDJSON in streaming sullo stesso percorso protetto: nessuna dipendenza nuova lato browser.
- **Decisione:**
  - envelope `delegation.progress` (stato `running`, payload `{delta}`) dal worker al Core quando la delega ha `stream: true` (aggiunta compatibile al contratto `delegation`);
  - `Worker::delegate_streaming` nel bridge;
  - `Jarvis::chat_streaming` con un destinatario dei frammenti;
  - endpoint `POST /api/chat/stream` (NDJSON), ammesso anche ai dispositivi associati con il permesso `chat`;
  - la dashboard mostra il testo in arrivo con un cursore e lo sostituisce con l'esito.
- **Conseguenze:**
  - i frammenti non passano dalla policy: sono testo del modello, mai azioni. Gli strumenti restano proposte valutate dal Core;
  - durante la generazione il Core resta occupato come prima (un turno alla volta);
  - le app native continuano a usare `POST /api/chat`.
- **Evidenza:** `core/server/tests/streaming.rs` (worker reale contro un Ollama sintetico: tre frammenti, poi esito identico, conversazione salvata, token misurati); E2E dell'interfaccia sul nuovo endpoint.
- **Documenti interessati:** AI_SYSTEM.md, INTERFACE_AND_DEVICES.md, ROADMAP.md, contratto `delegation`.

### ADR-0026 — Notifiche sul telefono con ntfy e Telegram

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («procedi con le notifiche esterne sul telefono»); RF-13, canali di notifica esterni indicati come lacuna.
- **Contesto:** le notifiche arrivavano solo nella campanella e come notifiche del browser, quindi a computer acceso e pagina aperta.
- **Opzioni:**
  - push nativo APNs/FCM: richiede account sviluppatore, server e app pubblicate;
  - email: SMTP e credenziali complesse, consegna lenta;
  - ntfy (HTTP semplice, app gratuita, server ospitabile in proprio) e Telegram (bot via HTTPS): nessun servizio di JARVIS da gestire.
- **Decisione:**
  - tabella `notify_channels`; token nell'archivio dei segreti (`channel:<id>`, portachiavi dove disponibile);
  - filtro per gravità minima (predefinita `warning`, che include le conferme in attesa);
  - testo escluso per impostazione: si inviano titolo e categoria;
  - HTTPS obbligatorio salvo loopback, nessun reindirizzamento, timeout di 10 secondi;
  - invio in un thread separato con un nuovo tentativo; contatori `notify.external.sent` e `notify.external.failed`; errori nel log senza URL (che per Telegram contiene il token);
  - pubblicazione ntfy in JSON per titoli UTF-8; collegamento facoltativo `JARVIS_PUBLIC_URL`;
  - API `GET/POST /api/notify/channels`, `DELETE /api/notify/channels/{id}`, `POST …/{id}/test`, solo per il proprietario, con audit;
  - le conferme non si danno dal telefono attraverso questi canali.
- **Conseguenze:**
  - ogni messaggio inviato passa da un servizio esterno: per questo il testo è escluso salvo scelta esplicita;
  - un argomento ntfy pubblico è leggibile da chi lo conosce: serve un nome difficile da indovinare (almeno 8 caratteri) oppure un server con token;
  - nessun limite di frequenza oltre alla deduplicazione delle notifiche.
- **Evidenza:**
  - `core/server/tests/channels.rs` con server ntfy e Telegram sintetici: configurazioni non sicure rifiutate, segreti mai restituiti, conferma inviata a entrambi, testo escluso su ntfy e incluso su Telegram per scelta, prova d'invio, rimozione con audit;
  - unitari in `channels.rs`;
  - E2E: canale aggiunto dall'interfaccia e prova ricevuta.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, SECURITY_AND_PRIVACY.md, README.md, ROADMAP.md.

### ADR-0027 — OAuth 2.0 con PKCE per i connettori

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («continua a implementare le funzionalità mancanti»); lacuna «OAuth per i connettori» nel checkpoint.
- **Contesto:** i connettori accettavano solo token o chiavi incollati. Molti servizi (Google, GitHub, Spotify, Microsoft) usano OAuth 2.0 e rilasciano token che scadono.
- **Opzioni:**
  - flusso «device code»: non supportato da tutti i servizi;
  - codice di autorizzazione con un server di callback esterno: espone JARVIS su Internet;
  - codice di autorizzazione con PKCE e redirect sul loopback del Core: profilo raccomandato per le app native (RFC 8252), senza servizi esterni.
- **Decisione:**
  - nuovo tipo `auth.type = "oauth2"` nel contratto `connector` (aggiunta compatibile) con `authorize_url`, `token_url`, `client_id` e `scopes`;
  - `token_url` deve stare tra gli `allowed_hosts` ed entrambi gli URL devono usare HTTPS (HTTP solo su loopback con la policy di prova);
  - `POST /api/connectors/{id}/oauth/start` genera `state` (256 bit) e `code_verifier` casuali, validi 10 minuti e una sola volta, e restituisce la pagina di consenso; l'interfaccia la apre nel browser di sistema;
  - `GET /api/connectors/oauth/callback`, solo da questo computer e senza token di sessione, è autenticato dallo `state`. Scambia il codice sull'endpoint dei token con le protezioni dei connettori (niente redirect, indirizzi privati bloccati, timeout) e mostra una pagina di esito;
  - access token e stato del rinnovo (refresh token, scadenza, eventuale segreto del client) sono cifrati nell'archivio dei segreti;
  - rinnovo automatico a 60 secondi dalla scadenza; revoca e disinstallazione cancellano tutto;
  - audit di avvio, consenso negato, scambio fallito, autorizzazione e revoca; contatori `connector.oauth.*`.
- **Conseguenze:**
  - il proprietario registra un'app OAuth presso il servizio con redirect `http://localhost:8765/api/connectors/oauth/callback` e scrive `client_id` nel manifest;
  - i servizi che richiedono un segreto del client lo ricevono da un campo dell'interfaccia: resta cifrato e non entra nel manifest;
  - la revoca presso il servizio (endpoint di revoca) non è automatica: JARVIS cancella i token locali.
- **Evidenza:**
  - `core/server/tests/oauth.rs` contro un server di autorizzazione sintetico che verifica PKCE: avvio, `state` falso e già usato, scambio, token mai restituiti, rinnovo automatico prima della chiamata, consenso negato, revoca, audit;
  - unitari in `oauth.rs` (vettore della RFC 7636, `state` monouso).
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, SECURITY_AND_PRIVACY.md, ACCEPTANCE.md, README.md, ROADMAP.md, contratto `connector`.

### ADR-0028 — Memoria con i sinonimi tramite embedding di Ollama

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («completalo definitivamente»); limite dichiarato dall'ADR-0022 («auto» non trova «macchina»).
- **Contesto:** il vettore locale riconosce flessioni e accenti, non il significato.
- **Opzioni:**
  - embedding calcolati nel worker Python: un viaggio in più sul bridge per ogni elemento;
  - embedding chiesti dal Core a Ollama (`POST /api/embed`): stesso servizio locale del modello di chat, nessuna dipendenza nuova.
- **Decisione:**
  - trait `Embedder` con implementazione Ollama (timeout 8 secondi, pausa di 60 secondi dopo un errore);
  - tabella derivata `memory_embeddings (id, model, vec)`, aggiornata da inserimento, correzione, cancellazione e ricostruzione; gli elementi già presenti vengono calcolati all'attivazione;
  - punteggio: 30% vettore locale, 25% BM25 e fino al 60% dal modello, considerando solo la similarità sopra 0,4;
  - modello scelto nella configurazione guidata (`nomic-embed-text` o `mxbai-embed-large`, scaricato con `ollama pull`) o con `JARVIS_EMBED_MODEL`;
  - stato in `/api/health` (`core.memory_embeddings`) e nella sezione Sistema.
- **Conseguenze:**
  - con Ollama spento la ricerca resta quella locale, senza errori;
  - cambiare modello rende i vettori precedenti inutilizzati finché non vengono ricalcolati (all'avvio o con «Ricostruisci indici»).
- **Evidenza:** unitari in `memory.rs`: sinonimo trovato solo con il modello, embedding degli elementi esistenti, correzione, cancellazione, ricostruzione; client Ollama contro un server sintetico, con pausa dopo un errore.
- **Documenti interessati:** MEMORY_DATA_SYNC.md, ACCEPTANCE.md, ROADMAP.md.

### ADR-0029 — Labiale con visemi allineati all'audio

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario; voce «sincronizzazione labiale basata sui fonemi» nel checkpoint e in INTERFACE_AND_DEVICES.md.
- **Contesto:** la bocca dell'avatar seguiva solo il volume, quindi ogni suono aveva la stessa forma.
- **Opzioni:**
  - allineamenti dei fonemi di Piper: disponibili solo con alcune voci e versioni;
  - riconoscimento dei fonemi dall'audio: un modello in più;
  - conversione grafema→visema dell'italiano, che è quasi fonetico, con tempi allineati all'audio.
- **Decisione:**
  - `ai/jarvis_ai/visemes.py` trasforma il testo in visemi (A, E, I, O, U, M, F, L, S, X) con pesi fonetici: vocali lunghe, consonanti brevi, pause alla punteggiatura;
  - i tempi coprono esattamente il parlato: inizio e fine dopo e prima dei silenzi, pause lunghe agganciate al silenzio più vicino, segmenti distribuiti fra le pause;
  - campo facoltativo `visemes` nel contratto `voice-synthesis`; `POST /api/voice/speak` con `with_visemes: true` restituisce audio e visemi insieme;
  - la dashboard imposta forma (`--mouth-width`, `data-viseme`) e apertura (visema × intensità dell'audio) a ogni fotogramma. L'avatar grande ha una bocca olografica, quello piccolo un'ellisse.
- **Conseguenze:**
  - l'allineamento è stimato, non misurato fonema per fonema: con frasi lunghe senza pause può anticipare o ritardare di qualche decina di millisecondi;
  - le parole straniere seguono la pronuncia italiana delle lettere.
- **Evidenza:** `tests/ai/test_voice.py` (sequenza, allineamento a silenzi e durata, contratto) e `tests/ui/audio_offline.test.cjs` (visema corrente, labbra chiuse, bocca arrotondata, ritorno a riposo).
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md, contratto `voice-synthesis`.

### ADR-0030 — Firma Ed25519 del registro di audit

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario; lacuna «firma dell'audit» nel checkpoint.
- **Contesto:** la catena SHA-256 rileva un evento modificato, ma chi può scrivere il file può ricalcolare l'intera catena, che torna coerente.
- **Opzioni:**
  - ancoraggio esterno degli hash (servizio di marcatura temporale): richiede rete e un terzo;
  - firma di ogni evento con una chiave locale tenuta fuori dal file: nessuna dipendenza esterna.
- **Decisione:**
  - chiave Ed25519 generata al primo avvio e custodita nell'archivio dei segreti (`audit:signing-key`, portachiavi su macOS e Windows);
  - ogni nuovo evento è firmato sul messaggio `jarvis-audit/1|stream|sequenza|hash`. Le firme stanno in `core-security.jsonl.sig.jsonl`, così gli eventi restano conformi al contratto;
  - `GET /api/audit` riporta `signatures`: firmati, eventi precedenti all'attivazione, validità, errori e chiave pubblica;
  - la sezione Audit mostra lo stato e la chiave pubblica da conservare fuori dal computer;
  - il backup cifrato include il file delle firme; la chiave privata viaggia con l'archivio dei segreti.
- **Conseguenze:**
  - una catena ricalcolata non ha firme valide, salvo che l'attaccante abbia anche la chiave privata, cioè accesso al portachiavi;
  - gli eventi scritti prima dell'attivazione restano protetti solo dalla catena di hash e sono contati a parte.
- **Evidenza:**
  - unitario `signatures_detect_a_recomputed_chain`: catena ricalcolata coerente ma firma non valida;
  - E2E: firme valide nella sezione Audit.
- **Documenti interessati:** SECURITY_AND_PRIVACY.md, ROADMAP.md.

### ADR-0031 — Revoca OAuth presso il servizio e scelta automatica del nodo

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario; limiti dichiarati dagli ADR-0027 (revoca solo locale) e ADR-0017 (assegnazione manuale ai nodi).
- **Decisione — revoca OAuth:**
  - campo facoltativo `auth.revoke_url` nel contratto `connector`, con host tra gli `allowed_hosts`;
  - alla revoca o disinstallazione il Core chiama l'endpoint RFC 7009 con il refresh token (o l'access token) prima di cancellare i token locali;
  - l'esito torna in `remote_revoked` e `remote_error`. I token locali vengono cancellati comunque.
- **Decisione — scelta del nodo:**
  - a ogni richiesta di lavoro il nodo dichiara CPU e, su Linux, carico medio e memoria disponibile;
  - lo strumento `node.auto.<strumento>` esiste finché almeno un nodo lo offre. Il Core sceglie, fra i nodi in linea che lo offrono, quello con meno lavori in corso, poi con il carico per CPU più basso, poi con più memoria libera;
  - policy, conferma («sul nodo scelto automaticamente») e audit restano quelli dei nodi;
  - in chat: «nodo auto <strumento> …»; il nome «auto» è riservato.
- **Conseguenze:**
  - su macOS e Windows il nodo dichiara solo le CPU: la scelta si basa su lavori in corso e CPU;
  - se il servizio non risponde alla revoca, l'interfaccia lo segnala; la revoca resta valida in JARVIS.
- **Evidenza:**
  - `tests/oauth.rs`: revoca RFC 7009 ricevuta dal server sintetico con il refresh token;
  - `tests/nodes.rs`: due nodi reali, esecuzione automatica, strumento rimosso alla revoca;
  - unitari in `nodes.rs`: priorità a carico, lavori in corso e revoca.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md, contratto `connector`.

### ADR-0032 — Attività e notifiche nelle app iOS e Android

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario; limite «funzioni iOS e Android oltre chat e conferme».
- **Contesto:** le app native mostravano solo chat e conferme, mentre il permesso `chat` del dispositivo consente già attività e notifiche.
- **Decisione:**
  - i client `CoreClient` (Swift e Kotlin) aggiungono `tasks`, `notices` e `markAllRead` sulle rotte già ammesse ai dispositivi (`GET /api/tasks`, `GET /api/notifications`, `POST /api/notifications/read-all`), con le stesse regole: solo HTTPS, nessun redirect, revoca con 401;
  - le app hanno tre schede: Chat, Attività (stato in italiano, errori evidenziati) e Notifiche (non lette in evidenza, contatore sulla scheda in iOS, «Segna lette»);
  - nessuna rotta nuova sul Core: memoria, automazioni e impostazioni restano al computer del proprietario.
- **Conseguenze:**
  - le notifiche si vedono aprendo l'app; il push a app chiusa passa da ntfy o Telegram (ADR-0026);
  - la voce resta nella PWA (stesse rotte vocali), non nelle app native.
- **Evidenza:** `CoreClientTest.tasksAndNoticesUseDeviceRoutesOnly` (Android, 6/6 in locale) e i casi aggiunti a `apps/ios/tests/CoreClientTests.swift`, eseguiti dalla CI su macOS; build dell'APK e dell'app per simulatore in CI.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0033 — Portachiavi su Linux con Secret Service

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario; limite dell'ADR-0021 (Linux solo con il file 0600).
- **Decisione:**
  - su Linux la chiave dei segreti va nel Secret Service (GNOME Keyring, KWallet) tramite `keyring` con D-Bus; libdbus è compilata dal sorgente, così la build non richiede pacchetti di sviluppo;
  - il portachiavi si usa solo con una sessione D-Bus (`DBUS_SESSION_BUS_ADDRESS`). Su server, servizi di sistema e CI resta il file 0600;
  - stesse regole dell'ADR-0021: spostamento dal file con rilettura, nessuna chiave nuova se il portachiavi non risponde e ci sono segreti, ripiego sul file solo senza segreti.
- **Conseguenze:**
  - su un desktop Linux con sessione grafica copiare la cartella dei dati non basta più per leggere i segreti;
  - avviare il Core senza la sessione con cui la chiave è stata salvata (per esempio da un servizio di sistema) richiede di ripristinare un backup o di usare `JARVIS_SECRETS_KEYCHAIN=off` fin dall'inizio.
- **Evidenza:** unitario `linux_without_secret_service_falls_back_to_the_file` (bus D-Bus non raggiungibile, ripiego sul file; senza sessione nessun portachiavi); CI «avvio reale» su Ubuntu (file).
- **Documenti interessati:** SECURITY_AND_PRIVACY.md, ACCEPTANCE.md, ROADMAP.md.

### ADR-0034 — Controllo aggiornamenti con canali Stable e Beta

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario; lacuna «canali Stable/Beta» in ACCEPTANCE.md.
- **Contesto:** il proprietario non sapeva quando usciva una nuova versione; l'installazione automatica non è possibile senza installer firmati con certificati commerciali.
- **Decisione:**
  - modulo `updates.rs`; impostazioni in `<dati>/updates.json`, **spente per impostazione** perché il controllo contatta GitHub;
  - con il controllo attivo, al massimo una richiesta al giorno all'elenco delle release (`JARVIS_UPDATE_URL`, predefinito il repository di JARVIS), fatta fuori dal blocco del Core, senza dati del proprietario e con timeout di 10 secondi;
  - Stable considera solo le release stabili, Beta anche le prerelease; il confronto segue le versioni semantiche;
  - una versione più recente diventa una notifica `system` deduplicata per versione, con il collegamento alla release; nessuna installazione automatica;
  - API `GET/POST /api/updates` (configurazione con audit) e `POST /api/updates/check` (controllo manuale); pannello «Aggiornamenti» nella sezione Sistema. Il collegamento delle notifiche ammette ora anche le viste Sistema, Dispositivi e Plancia (aggiunta compatibile al contratto).
- **Conseguenze:**
  - con il repository privato l'elenco non è leggibile senza credenziali: il controllo riporta l'errore e non notifica nulla;
  - l'aggiornamento resta manuale: si scarica la release, si verifica la firma (ADR-0020) e si installa.
- **Evidenza:** unitari in `updates.rs` (versioni, canali, impostazioni, cadenza giornaliera, errori); `update_check_notifies_once_per_version` in `tests/api.rs`; E2E: spento per impostazione, controllo manuale verso un elenco sintetico con versione disponibile.
- **Documenti interessati:** ACCEPTANCE.md, README.md, ROADMAP.md, contratto `notification`.

### ADR-0035 — Rifinitura grafica della dashboard per macOS e Windows

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («migliora la grafica della dashboard sia per macOS che per Windows»). Estende l'ADR-0023.
- **Decisione:**
  - icone vettoriali in linea (`Icon.tsx`), senza dipendenze o richieste esterne;
  - barra laterale a gruppi (Operazioni, Dati, Integrazioni, Sistema), riducibile e ricordata nel browser, con il numero di conferme in attesa sulla Chat;
  - palette comandi con ⌘K su macOS e Ctrl+K su Windows e Linux: sezioni, comandi rapidi e richiesta libera a JARVIS;
  - Plancia con saluto, riga di indicatori (attività di oggi, riuscite, da confermare, ricordi, nodi in linea), ingresso scaglionato dei pannelli e sfondo con luci in movimento lento;
  - chat con etichette «Tu»/«JARVIS» via CSS, indicatore di scrittura animato e pulsanti con icona; notifiche a comparsa con icona e barra del tempo;
  - adattamento alla piattaforma: classe `platform-mac` o `platform-win` sulla radice; caratteri SF su macOS e Segoe UI Variable su Windows; raggi diversi; barre di scorrimento sottili;
  - app desktop: finestra scura con sfondo coerente fin dall'apertura, parametro `?app=desktop` per le rifiniture dell'app, schermata d'avvio con reattore animato e stato d'errore.
- **Conseguenze:** il movimento rispetta `prefers-reduced-motion`; gli identificativi del DOM usati dai test restano invariati; nessuna nuova rotta né contratto.
- **Evidenza:** `tsc` e `npm run build` OK; E2E `tests/e2e/ui_smoke.cjs` superato con il Core 1.3.0 offline; `cargo fmt`, `clippy -D warnings` e `cargo check` dell'app Tauri OK; schermate di Plancia, Chat e palette controllate.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0036 — Icona, installatori e finestra dell'app per macOS e Windows

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («migliora la grafica dell'app macOS e il file di installazione, fai sì che funzioni e migliora anche per Windows»). Estende ADR-0016, ADR-0020 e ADR-0035.
- **Decisione:**
  - nuova icona disegnata in modo riproducibile da `scripts/make_brand_assets.py` (Pillow, nessuna risorsa esterna): lastra arrotondata scura con emblema a reattore; su macOS margini e ombra della griglia Apple, su Windows e Linux lastra a tutta superficie;
  - `.dmg` impaginato: sfondo con istruzioni («Trascina JARVIS nella cartella Applicazioni», percorso «Apri comunque»), lastra chiara dietro le icone perché le etichette di Finder restino leggibili in modalità chiara e scura, posizioni fisse di app e Applicazioni. In CI la build macOS gira con `CI=false` (altrimenti Tauri salta l'impaginazione) e ripiega sul `.dmg` semplice se Finder non risponde;
  - macOS 11 come versione minima dichiarata; barra del titolo trasparente senza testo sopra lo sfondo scuro della finestra;
  - all'avvio, dopo l'autorizzazione del proprietario, l'app toglie l'attributo di quarantena dal proprio pacchetto in `/Applications`, così Core e Python inclusi non vengono bloccati come programmi scaricati; nessun effetto se l'app gira dal `.dmg` montato o da una copia traslocata;
  - Windows: setup NSIS in italiano con immagini laterale e d'intestazione, icona dell'installer e cartella del menu Start «JARVIS»; MSI con banner e immagine di dialogo; finestra con tema scuro (ADR-0035);
  - note di installazione per ogni sistema in testa alla release (`.github/release-notes.md`); la verifica in CI controlla anche il sigillo profondo (`codesign --deep --strict`) e il collegamento ad Applicazioni nel `.dmg`.
- **Conseguenze:** senza certificati commerciali restano gli avvisi di Gatekeeper e SmartScreen al primo avvio, ora spiegati nel `.dmg` e nella release. La versione passa a 1.3.2.
- **Evidenza:** anteprima delle risorse generate; `cargo fmt` e `clippy -D warnings` dell'app su Linux; build degli installatori e prove di installazione nel workflow `release` sui 3 sistemi (il codice solo macOS si compila lì).
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md, README.md.

### ADR-0037 — Ciclo dell'agente, prompt di sistema, OAuth reattivo, database e memoria senza duplicati

- **Data:** 2026-11-18 · **Stato:** accettato · **Fonte:** richiesta del proprietario («system prompt del modello, migliora l'orchestratore e il collegamento ai tools e agli OAuth, migliora il database e la memoria»). Estende ADR-0004, ADR-0022, ADR-0024 e ADR-0027.
- **Contesto:** l'orchestratore eseguiva un solo strumento per richiesta, nonostante il limite `max_steps: 4` dichiarato nella delega; il prompt di sistema non conosceva data e ora né regole d'uso degli strumenti; un token OAuth invalidato dal servizio prima della scadenza falliva fino al rinnovo successivo; la memoria accettava duplicati; il database non aveva manutenzione né diagnostica.
- **Decisione:**
  - **Ciclo dell'agente** (`Jarvis::drive`): fino a 4 strumenti per richiesta. Ogni proposta passa da policy, audit ed eventuale conferma; dopo ogni risultato l'agente riceve `inputs.tool_history` e `inputs.steps_left` (aggiunte compatibili al contratto di delega) e decide se chiedere un altro strumento o rispondere. Con `steps_left = 0`, dopo un rifiuto o una negazione della policy l'agente deve rispondere. Una proposta identica a un passo già eseguito chiude il ciclo (`loop_guard`). La conferma sospende il ciclo e lo riprende con lo stesso contesto. Gli esiti precedenti all'ultimo viaggiano ridotti a 3.000 caratteri.
  - **Prompt di sistema** (`system_prompt` in `gateway.py`): identità, data e ora locali, stile, regole sugli strumenti (uno per volta, mai risultati inventati, conferma chiesta dal Core, memoria prima di `memory.search`, salvataggio dei fatti stabili), sicurezza sui contenuti non attendibili, preferenze del proprietario come istruzioni. I passi precedenti tornano al modello come chiamate e risultati di strumenti; anche i file letti sono marcati come non attendibili.
  - **OAuth reattivo:** una risposta 401 diventa `connector.token_rejected`; con OAuth il Core rinnova subito il token e ripete la chiamata una sola volta, anche per le scritture, perché il servizio non le ha eseguite.
  - **Memoria:** `memory.remember` riconosce lo stesso fatto (maiuscole, accenti e punteggiatura ignorati) e restituisce l'elemento esistente con `already_known`; gli episodi elencano tutti gli strumenti del turno.
  - **Database:** `synchronous=NORMAL` con WAL, `busy_timeout` di 5 secondi, `foreign_keys`, temporanei in memoria; indici su task, messaggi e scadenze delle automazioni; versione dello schema in `user_version` (2); `PRAGMA optimize` all'apertura e manutenzione ogni 6 ore (statistiche, checkpoint del WAL, pulizia dei ricordi scaduti); `GET /api/database` con integrità rapida, dimensione e righe, mostrato nel pannello «Database locale» della sezione Sistema.
- **Conseguenze:** una richiesta composta («che ore sono e che sistema uso?») si risolve in un turno; il consumo di token cresce con i passi, limitato da `steps_left` e dalla riduzione degli esiti. In modalità offline il comportamento non cambia (un solo strumento). Versione 1.4.0.
- **Evidenza:** `tests/agent_loop.rs` (catena di 2 strumenti, ripetizione bloccata, arresto a 4 strumenti) con il worker reale contro un Ollama sintetico; `tests/oauth.rs` (401, rinnovo e nuova chiamata); `tests/memory.rs` (nessun duplicato, `/api/database`); `tests/ai/test_gateway.py` (strumenti di nuovo offerti, risposta finale senza strumenti, prompt con data e preferenze); E2E 71/71.
- **Documenti interessati:** AI_SYSTEM.md, AUTONOMY_AND_TOOLS.md, MEMORY_DATA_SYNC.md, ROADMAP.md, contratto `delegation`.

### ADR-0038 — Processi, terminale con conferma e specializzazioni degli agenti

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** richiesta del proprietario («continua a migliorare JARVIS e terminalo completamente»); lacune RF-05 («niente desktop o terminale») e RF-03 («agente generalista unico») in ACCEPTANCE.md.
- **Decisione:**
  - **`process.list`** (lettura, rischio basso, senza conferma): processi ordinati per CPU o memoria con `ps` su macOS e Linux e `tasklist` su Windows; disponibile anche sui nodi associati.
  - **`shell.run`** (rischio critico, conferma a ogni comando): comando eseguito con la shell del sistema nella workspace di JARVIS, senza le variabili `JARVIS_*`, stdin chiuso, output limitato a 8.000 caratteri per flusso e interruzione dopo 60 secondi. La conferma mostra il comando esatto. La policy nega comandi distruttivi o con privilegi noti (`sudo`, `rm -rf /`, `mkfs`, `dd if=`, `shutdown`, `format c:`…) con `policy.command_blocked`, e tutto il terminale con `JARVIS_SHELL=off`. Il terminale non è ammesso nelle automazioni né sui nodi.
  - **Specializzazioni** (`agents.rs`): il Core sceglie `research`, `system`, `automation`, `memory` o `coding` quando la richiesta ricade in un solo ambito, altrimenti il generalista. Lo specialista riceve solo gli strumenti del proprio ambito più ora e ricerca nella memoria, e il proprio ruolo nel prompt di sistema (`delegate.role`). Il worker scarta le proposte fuori ambito; in seconda difesa il Core le registra come `scope` e non le passa alla policy. La specializzazione appare come etichetta nella chat.
- **Conseguenze:** meno strumenti descritti al modello per le richieste specialistiche (meno token, meno errori di scelta). Il terminale resta opzionale e sempre sotto conferma; desktop e mouse restano esclusi. Versione 1.5.0.
- **Evidenza:** `tests/terminal.rs` (processi senza conferma; comando eseguito solo dopo la conferma, con effetto nella workspace e voce di audit; comandi distruttivi negati senza conferma; terminale rifiutato nelle automazioni); `tests/agent_loop.rs` (lo specialista della ricerca riceve solo i suoi strumenti e il ruolo); unitari di `agents.rs`; `tests/ai` (ruolo nel prompt); E2E 73/73.
- **Documenti interessati:** ACCEPTANCE.md, REQUIREMENTS.md, AUTONOMY_AND_TOOLS.md, SECURITY_AND_PRIVACY.md, AI_SYSTEM.md, ROADMAP.md.

### ADR-0039 — Traccia di ogni task (W3C Trace Context)

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** richiesta del proprietario («continua con le funzionalità mancanti»); lacuna RNF-07 «trace distribuite assenti» in ACCEPTANCE.md.
- **Decisione:**
  - ogni task ha una traccia con identificativi compatibili con W3C Trace Context: `trace_id` = `correlation_id` senza trattini (32 esadecimali), intervallo radice = prime 16 cifre del `task_id`, intervalli figli con id casuali di 16 cifre;
  - intervalli registrati dal Core: delega all'agente (specializzazione, backend, token, passi residui), decisione della policy (rischio, motivi), esecuzione dello strumento (tipo `tool`, `web`, `connector`, `node`, `terminal`; codice d'errore ed effetto), attesa della conferma del proprietario (approvata o rifiutata) e richiesta completa;
  - nessun contenuto dell'utente: solo nomi di strumenti, tempi, esiti e contatori;
  - tabella `task_spans` (schema v3), conservazione di 30 giorni con la manutenzione periodica; `GET /api/tasks/{id}/trace`;
  - sezione Attività: «Mostra traccia» apre la cronologia con barre proporzionali alla durata e colori per esito.
- **Conseguenze:** si vede dove si spende il tempo di una richiesta (modello, strumenti, connettori, attesa della conferma). Le tracce restano locali: nessun esportatore OTLP verso servizi esterni. Versione 1.6.0.
- **Evidenza:** `every_task_has_a_w3c_trace_with_agent_policy_and_tool_spans` in `tests/api.rs` (identificativi, gerarchia, assenza di contenuti, intervallo di conferma, 404 per task senza traccia); E2E con la cronologia nella sezione Attività.
- **Documenti interessati:** ACCEPTANCE.md, DEVOPS_QA_OBSERVABILITY.md, ROADMAP.md.

### ADR-0040 — Agenti temporanei

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** richiesta del proprietario («procedi con gli agenti temporanei»); lacuna RF-03 «niente agenti temporanei né sottodeleghe».
- **Decisione:**
  - nuovo strumento `agent.delegate` (`role`, `objective`, `tools` facoltativi; rischio medio, senza conferma): l'agente principale affida un sotto-compito a un agente temporaneo;
  - l'agente temporaneo è un task figlio (`parent_task_id`, stessa `correlation_id`, richiedente `agent`) con delega `agent: "temporary"`, ruolo nel prompt, al massimo 3 strumenti e nessuna sottodelega; il principale ne può creare al massimo 2 per richiesta (`limits.max_subdelegations`);
  - strumenti concessi: solo quelli senza conferma e non ad alto rischio (predefiniti di sola lettura: ora, sistema, file in lettura, ricerca nella memoria, Web); una richiesta con strumenti non concedibili viene rifiutata con `agent.invalid_tools` prima di creare il figlio;
  - ogni strumento del figlio passa da ambito, policy e audit (soggetto `agent/temporary`); ciò che richiederebbe una conferma viene negato e resta all'agente principale;
  - traccia unica: la radice del figlio è figlia della radice del principale e i suoi intervalli stanno nella stessa traccia (ADR-0039);
  - interfaccia: etichetta «agente temporaneo · ruolo» in chat; nella sezione Attività il figlio indica il task principale.
- **Conseguenze:** l'agente principale può scomporre una richiesta in sotto-compiti di sola lettura senza aumentare i permessi. Le esecuzioni sono sequenziali (il Core resta single-thread). Versione 1.7.0.
- **Evidenza:** `tests/subagents.rs` (figlio con ruolo, stessa correlazione e traccia; nessuna scrittura né conferma); `temporary_agents_only_get_tools_without_confirmation` in `tests/agent_loop.rs`; E2E 75/75.
- **Documenti interessati:** ACCEPTANCE.md, AI_SYSTEM.md, AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0041 — mTLS fra Core e nodi con certificati fissati per impronta

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** scelta del proprietario («mTLS Core–nodi»); lacuna RF-11 e RNF-04 «mTLS assente» in ACCEPTANCE.md.
- **Contesto:** i nodi si autenticavano solo con il token del dispositivo, sopra l'HTTPS fornito da Tailscale. Un token copiato bastava per prendere il posto del nodo.
- **Opzioni:** autorità di certificazione locale con richieste di firma; certificati autofirmati fissati per impronta. Scelta la seconda: nessuna CA da custodire, revoca immediata dal registro dei dispositivi, nessun problema di nomi host.
- **Decisione:**
  - modulo `tls.rs` (rustls con provider ring, rcgen): Core e nodo generano ciascuno chiave ECDSA P-256 e certificato autofirmato; l'identità del Core sta in `<dati>/tls` (chiave 0600);
  - all'associazione il nodo invia solo il certificato (`cert_pem`); il Core ne registra l'impronta SHA-256 nel dispositivo (`cert_sha256`, aggiunta compatibile al contratto) e risponde con porta TLS e impronta del proprio certificato;
  - la porta TLS dei nodi è attiva solo con `JARVIS_NODE_TLS_ADDR` (es. `0.0.0.0:8766`) ed espone solo `/api/nodes/next` e `/api/nodes/result/{id}`; il certificato client è obbligatorio e ammesso solo se registrato; il nodo accetta solo il Core con l'impronta fissata, qualunque sia il nome host;
  - un nodo legato a un certificato deve presentare token **e** certificato: il token da solo viene rifiutato anche in chiaro; dopo la revoca il nodo completa l'handshake, riceve 401 e si ferma;
  - senza `JARVIS_NODE_TLS_ADDR` e per i nodi associati prima, il comportamento resta quello dell'ADR-0017.
- **Conseguenze:** furto del token inutile senza la chiave privata del nodo; nessuna dipendenza da Tailscale per la cifratura. La porta va resa raggiungibile dai nodi (rete privata o firewall). Versione 1.8.0.
- **Evidenza:** `tests/node_tls.rs` (associazione con certificato, chiave privata mai inviata, esecuzione via TLS, token senza certificato rifiutato, certificato estraneo rifiutato all'handshake, Core con impronta diversa rifiutato dal nodo, revoca che ferma il nodo); unitari di `tls.rs`.
- **Documenti interessati:** ACCEPTANCE.md, SECURITY_AND_PRIVACY.md, INTERFACE_AND_DEVICES.md, ROADMAP.md, contratto `device`.

### ADR-0042 — Passkey per le approvazioni dai dispositivi associati

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** scelta del proprietario («Passkey»); lacuna RNF-04 «passkey assenti» in ACCEPTANCE.md.
- **Contesto:** un dispositivo associato con permesso di conferma approvava le azioni rischiose con il solo token salvato nel browser: chi ottiene il telefono sbloccato o copia il token può approvare.
- **Decisione:**
  - WebAuthn con chiave ES256 (P-256), verifica dell'utente obbligatoria (impronta, volto o PIN), attestazione «none»; RP = host da cui il dispositivo raggiunge il Core, origine `https://` o `http://` dello stesso host;
  - verifica nel Core senza nuove librerie crittografiche (`passkeys.rs`: CBOR minimo, `authenticatorData`, chiave COSE, firma con ring); tabella `passkeys` (schema v4) con sola chiave pubblica e contatore;
  - sfide monouso di 5 minuti, legate al dispositivo e allo scopo (registrazione, oppure approvazione di una conferma precisa);
  - da un dispositivo con passkey registrata, `POST /api/confirmations/{id}` con `approve: true` richiede un'asserzione valida per quella conferma (`passkey_required`, `passkey_invalid`); rifiutare non la richiede; il proprietario sul computer del Core non ne ha bisogno;
  - contatore non crescente rifiutato (passkey clonata); registrazione, uso, rifiuti ed eliminazione nell'audit;
  - rotte: `GET /api/passkeys`, `POST /api/passkeys/register/options`, `POST /api/passkeys/register`, `POST /api/passkeys/assert/options`, `DELETE /api/passkeys/{id}` (solo il proprietario);
  - interfaccia: sul dispositivo «Proteggi le approvazioni con una passkey» in chat; «Approva» chiede la passkey quando serve; un 401 da passkey non disconnette il dispositivo.
- **Conseguenze:** il token da solo non basta più ad approvare dal dispositivo protetto. WebAuthn richiede un contesto sicuro: HTTPS (es. `tailscale serve`) o `localhost`. Versione 1.9.0.
- **Evidenza:** unitari di `passkeys.rs` (andata e ritorno, firma alterata, verifica dell'utente assente, sfida/origine/RP diversi, contatore, CBOR malformato); `tests/passkeys.rs` con autenticatore sintetico (passkey obbligatoria, sfida monouso e legata alla conferma, chiave diversa rifiutata, rifiuto libero, gestione del proprietario, audit); E2E con l'autenticatore virtuale di Chromium sul telefono simulato (`jarvis.localhost`), 77/77.
- **Documenti interessati:** ACCEPTANCE.md, SECURITY_AND_PRIVACY.md, INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0043 — Navigazione Web interattiva con il browser del sistema

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** scelta del proprietario («Navigazione Web»); lacuna RF-06 «navigazione interattiva assente» in ACCEPTANCE.md.
- **Opzioni:** includere un Chromium nell'app (oltre 150 MB per sistema, aggiornamenti di sicurezza a carico di JARVIS); usare Playwright tramite Python; pilotare direttamente il browser già installato con il DevTools Protocol. Scelta l'ultima: nessun download, browser aggiornato dal sistema, Edge presente su ogni Windows.
- **Decisione:**
  - modulo `browser.rs`: avvia Chrome, Chromium o Edge (oppure `JARVIS_BROWSER`) in headless con un profilo temporaneo cancellato alla fine, senza estensioni né sincronizzazione, e lo pilota via WebSocket locale (`tungstenite`, senza TLS);
  - ogni richiesta di rete della pagina (sotto-risorse comprese) è intercettata (`Fetch.requestPaused`) e passa dalla policy Web: domini consentiti e negati, indirizzi privati vietati anche dopo la risoluzione DNS; le richieste bloccate sono riportate nell'esito;
  - `web.browse` (rischio medio, senza conferma): apre la pagina eseguendo JavaScript, segue fino a 3 collegamenti per testo o indice, restituisce titolo, testo visibile (20.000 caratteri), collegamenti, moduli con i nomi dei campi, ora e SHA-256 del testo;
  - `web.form` (rischio alto, conferma obbligatoria): compila un modulo per nome dei campi e lo invia; la conferma mostra indirizzo e campi, con i valori di password, token, carte e IBAN mascherati;
  - contenuto marcato come non attendibile per il modello; fonti allegate alla risposta come per `web.fetch`; lo specialista della ricerca riceve anche i due nuovi strumenti;
  - comandi offline: «naviga <url> e clicca <testo>», «compila modulo <url>: campo=valore, …».
- **Conseguenze:** pagine dinamiche e moduli diventano utilizzabili senza installare nulla oltre a un browser comune. Il DNS risolto dal browser può in teoria differire da quello controllato dal Core (finestra di rebinding): resta mitigato dal controllo per richiesta e dal divieto degli host privati. Niente sessioni persistenti né accessi con le credenziali del proprietario. Versione 1.10.0.
- **Evidenza:** `tests/browser.rs` con il browser reale contro un sito sintetico (JavaScript eseguito, collegamento seguito, modulo inviato, immagine verso host negato bloccata e mai ricevuta, campo inesistente, indirizzo privato rifiutato prima dell'avvio; dalla chat: lettura senza conferma con fonte, invio solo dopo la conferma con password mascherata); in CI il browser è obbligatorio (`google-chrome` dei runner Ubuntu).
- **Documenti interessati:** ACCEPTANCE.md, AUTONOMY_AND_TOOLS.md, SECURITY_AND_PRIVACY.md, ROADMAP.md.

### ADR-0044 — App desktop nativa: barra dei menu, scorciatoia globale e notifiche di sistema

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** richiesta del proprietario («migliora la grafica dell'app Windows e soprattutto dell'app macOS»).
- **Decisione:**
  - icona nella barra dei menu (macOS) o nell'area di notifica (Windows, Linux) con «Apri JARVIS», «Nascondi», «Esci da JARVIS»; clic sinistro per mostrare o nascondere;
  - su macOS menu dell'applicazione in italiano (Informazioni, Nascondi, Modifica con copia e incolla, Finestra, Schermo intero) e riapertura dal Dock;
  - chiudere la finestra la nasconde: voce, automazioni e notifiche restano attive; solo «Esci da JARVIS» ferma Core e worker;
  - scorciatoia globale ⌘⇧J (macOS) o Ctrl+Maiusc+J (Windows, Linux) per mostrare o nascondere JARVIS da qualunque app; istanza singola; posizione e dimensioni della finestra ricordate;
  - notifiche native del sistema: l'app legge le notifiche non lette del Core ogni 10 secondi e mostra le nuove quando la finestra non è in primo piano (la WebView di macOS non offre le notifiche Web); il launcher genera il token di sessione (128 bit) e lo passa al Core con `JARVIS_TOKEN`;
  - dashboard nell'app: su macOS barra laterale e intestazione traslucide, raggi morbidi; su Windows superfici piene in stile Fluent con indicatore della voce attiva; interfaccia non selezionabile come testo.
- **Conseguenze:** JARVIS si comporta come un assistente residente. La scorciatoia può essere già occupata da un'altra app: in quel caso resta l'icona. La prova P7 è aggiornata.
- **Evidenza:** `cargo clippy -D warnings` e build bloccata dell'app su Linux; build dell'app su macOS e Windows nella CI; `launches_core_on_free_port_and_stops_it` verifica che il token del launcher apra le notifiche del Core.
- **Documenti interessati:** ACCEPTANCE.md, INTERFACE_AND_DEVICES.md, ROADMAP.md.

### Decisione operativa — Merge automatico con CI verde

- **Data:** 2026-11-17 · **Fonte:** richiesta esplicita del proprietario in sessione («merge sempre se CI verde automaticamente»).
- **Regola:** dettagliata in [DEVELOPMENT.md](DEVELOPMENT.md#integrazione-delle-modifiche).

## Aggiornamento del registro

Per un nuovo ADR registrare identificativo, data, stato, contesto, opzioni, decisione, motivazione, conseguenze, evidenza e documenti interessati. Indicare esplicitamente quando una nuova scelta sostituisce una precedente. Se si recupera una risposta storica, collegare la fonte e verificare domanda, scelta e precisazioni prima di aggiungerla.
