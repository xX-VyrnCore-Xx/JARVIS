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

- **Data:** 2026-11-17 · **Stato:** Accettato (Fase 4, RF-14). Resta aperta la custodia della chiave nel portachiavi del sistema operativo.
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
  - nessun OAuth, nessuna paginazione, nessun metodo oltre `GET` e `POST`;
  - la chiave dei segreti sta sul disco accanto ai dati: protegge da letture del database, non da chi legge l'intera cartella. Da spostare nel portachiavi del sistema (decisione aperta);
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
- **Custodia della chiave:** il portachiavi del sistema operativo resta da fare. Richiede prove su Windows, macOS e Linux con sessione grafica, non disponibili nel container né nella CI, ed è rinviato alla validazione di sicurezza della Fase 6 sul computer del proprietario.

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

- **Data:** 2026-11-17 · **Stato:** Accettato. Restano decisioni del proprietario: firma dei binari e degli installatori, canali Stable/Beta. Prima release: `v0.1.0`, richiesta dal proprietario e pubblicata subito, prima delle prove P1–P12 (prototipo, non 1.0.0).
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

### Decisione operativa — Merge automatico con CI verde

- **Data:** 2026-11-17 · **Fonte:** richiesta esplicita del proprietario in sessione («merge sempre se CI verde automaticamente»).
- **Regola:** dettagliata in [DEVELOPMENT.md](DEVELOPMENT.md#integrazione-delle-modifiche).

## Aggiornamento del registro

Per un nuovo ADR registrare identificativo, data, stato, contesto, opzioni, decisione, motivazione, conseguenze, evidenza e documenti interessati. Indicare esplicitamente quando una nuova scelta sostituisce una precedente. Se si recupera una risposta storica, collegare la fonte e verificare domanda, scelta e precisazioni prima di aggiungerla.
