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
  - il callback OAuth è autenticato dallo `state` monouso. La route API generica è `GET /api/connectors/oauth/callback`; Google Desktop usa il redirect loopback `http://127.0.0.1:<porta>` senza percorso e la radice gestisce solo callback con `state`, restando 404 per le altre richieste;
  - access token e stato del rinnovo (refresh token, scadenza, eventuale segreto del client) sono cifrati nell'archivio dei segreti;
  - rinnovo automatico a 60 secondi dalla scadenza; revoca e disinstallazione cancellano tutto;
  - audit di avvio, consenso negato, scambio fallito, autorizzazione e revoca; contatori `connector.oauth.*`.
- **Conseguenze:**
  - il proprietario registra un'app OAuth presso il servizio con il redirect richiesto dal provider e scrive `client_id` nel manifest; Google Desktop richiede l'IP loopback senza percorso;
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
- **Contesto:** il proprietario non sapeva quando usciva una nuova versione; l'installazione automatica non è affidabile su tutti i sistemi (Android richiede conferma di sistema).
- **Decisione:**
  - modulo `updates.rs`; impostazioni in `<dati>/updates.json`, controllo automatico attivo per impostazione e disattivabile dall'interfaccia;
  - ogni ora il Core interroga l'elenco release (`JARVIS_UPDATE_URL`, predefinito `xX-VyrnCore-Xx/JARVIS`), fuori dal blocco principale e senza dati del proprietario; timeout 10 secondi;
  - una repository pubblica non richiede credenziali; per una repository privata il proprietario configura un token fine-grained GitHub con permesso **Contents: read**, conservato cifrato nel secret store locale del Core. Non viene restituito dall'API, scritto nei log o inviato ai client;
  - quando è configurato un token, l'endpoint viene validato come HTTPS `api.github.com/repos/{owner}/{repo}/releases` e i redirect sono disabilitati per non inoltrare la credenziale;
  - Stable considera solo le release stabili, Beta anche le prerelease; il confronto segue le versioni semantiche;
  - una versione più recente diventa una notifica `system` deduplicata per versione, con il collegamento alla release; nessuna installazione automatica;
  - API `GET/POST /api/updates` (configurazione con audit, secret escluso dallo stato) e `POST /api/updates/check` (controllo manuale); sezione «Aggiornamenti» nell'app desktop proprietario.
- **Conseguenze:**
  - la notifica è condivisa tramite il Core; la release privata si apre nel browser con l'accesso GitHub dell'utente;
  - il download e l'installazione restano manuali: l'utente verifica la firma (ADR-0020) e Android richiede conferma di sistema.
- **Evidenza:** test unitari su versioni/canali/attivazione e cadenza oraria; test API per stato, memorizzazione/rimozione e mancata restituzione del token; test sintetico per `Authorization: Bearer`.
- **Documenti interessati:** ACCEPTANCE.md, README.md, ROADMAP.md, `GITHUB_ACTIONS_SECRETS.md`, contratto `notification`.

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

### ADR-0045 — Tool calling più robusto e modelli consigliati

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** richiesta del proprietario («migliora il modello di JARVIS e la tool calling»).
- **Decisione:**
  - argomenti non validi: il Core restituisce all'agente i difetti (percorso del campo e motivo, al massimo 4) e lo schema atteso; il turno non si chiude e l'agente può ripetere la chiamata corretta (il blocco delle ripetizioni resta per le chiamate identiche);
  - correzione dei tipi prima dell'invio al Core: numeri e booleani scritti come testo (anche «0,5», «sì»), oggetti o liste in forma di stringa JSON, valore singolo per un campo lista, campi non previsti scartati quando lo schema non li ammette; il Core valida comunque;
  - chiamate scritte nel testo invece che nel campo dedicato: `<tool_call>{…}</tool_call>`, blocco ```json``` o oggetto con `name` e `arguments`/`parameters`, accettate solo per strumenti delegati;
  - modelli proposti nella configurazione guidata: `qwen3:4b`, `qwen3:8b`, `llama3.1:8b` in testa ai precedenti; raccomandazione automatica in base alla memoria (≥ 16 GB → `qwen3:8b`, altrimenti `qwen3:4b`).
- **Conseguenze:** meno turni persi per errori di formato dei modelli locali piccoli.
- **Evidenza:** `invalid_arguments_are_explained_and_the_model_can_retry` in `tests/agent_loop.rs`; `test_arguments_are_coerced_to_the_schema`, `test_tool_calls_written_in_text_are_recognised` in `tests/ai`; unitario della raccomandazione in `setup.rs`.
- **Documenti interessati:** AI_SYSTEM.md, ROADMAP.md.

### ADR-0046 — Strumenti quotidiani: calcolo, unità, date, note, cose da fare, file, dischi, meteo

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** richiesta del proprietario («migliora la sua memoria e i suoi tools e aggiungine di nuovi tanti»).
- **Decisione:** 17 nuovi strumenti nel catalogo (38 in totale), tutti con policy, audit e tracce:
  - `math.calc` (parser a discesa ricorsiva, niente eval: operatori, potenza, percentuale, funzioni, costanti), `units.convert` (lunghezza, massa, volume, velocità, area, dati, tempo, temperatura), `date.calc` (giorni fra date, somma di giorni e mesi, giorno della settimana);
  - `notes.add/list/search/delete` e `todo.add/list/done` in SQLite (schema v5); le cose da fare con scadenza generano una notifica una sola volta (nuova categoria `reminder`, aggiunta compatibile al contratto);
  - `fs.search` (nome e testo, senza seguire collegamenti simbolici, al massimo 5.000 voci), `fs.mkdir`, `fs.move` e `fs.delete` con conferma (cartelle solo se vuote, mai la workspace stessa, percorsi fuori negati dalla policy);
  - `system.disks`; `weather.forecast` con Open-Meteo (senza chiavi, solo i suoi host, negato con il Web spento);
  - `memory.update` per correggere un ricordo per id;
  - selezione degli strumenti per pertinenza (parole chiave per strumento, fino a 12 descrizioni), limite del contratto di delega portato a 64 strumenti (aggiunta compatibile); nuovi strumenti assegnati agli specialisti; comandi offline in italiano per tutti.
- **Conseguenze:** JARVIS copre le richieste quotidiane senza modelli remoti né servizi a pagamento.
- **Evidenza:** unitari di `toolbox.rs` (calcolatore, unità, date, file, dischi); `tests/everyday.rs` (dalla chat: calcolo, conversione, note, cose da fare con notifica unica, file con conferma, percorsi fuori negati, dischi; meteo contro un Open-Meteo sintetico e negato con il Web spento); `tests/ai`; E2E 77/77.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, MEMORY_DATA_SYNC.md, ROADMAP.md, contratti `delegation` e `notification`.

### ADR-0047 — Catalogo dei connettori: Gmail, Google Calendar, Outlook, GitHub, Todoist, Slack

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** richiesta del proprietario («fai sì che si possa collegare Gmail e Calendar e tutto il resto»); decisione 81 (opzione D: framework completo con catalogo iniziale prioritario), il cui catalogo concreto viene così definito dal proprietario.
- **Decisione:**
  - modelli di manifest in `contracts/catalog/connectors/` incorporati nel Core, con indice (fornitore, categoria, tipo di accesso, istruzioni di configurazione e collegamento alla console):
    - **Gmail** (OAuth Google): cerca, leggi, etichette, bozza e invio con conferma;
    - **Google Calendar** (OAuth Google): eventi, calendari, nuovo evento con conferma;
    - **Outlook** (OAuth Microsoft, Graph): posta, invio, eventi, nuovo evento;
    - **GitHub** (token personale): repository, issue, notifiche, nuova issue;
    - **Todoist** (token, API v1): attività, nuova attività, completamento;
    - **Slack** (token del bot): canali, messaggi, invio;
  - l'OAuth usa il motore dell'ADR-0027 (PKCE, token cifrati, rinnovo e revoca); l'ID client è quello creato dal proprietario nella console del servizio (JARVIS non distribuisce credenziali proprie), il client secret di Google si inserisce all'accesso;
  - nuovo campo facoltativo `body_encoding` delle operazioni (aggiunta compatibile al contratto): con `gmail_raw` e `gmail_draft` il Core costruisce un messaggio RFC 822 UTF-8 da `to`, `subject`, `body`, `cc` (oggetto in codifica MIME, a capo nelle intestazioni rifiutati) e lo codifica in base64url, invece di chiederlo al modello;
  - `GET /api/connectors/templates`, `POST /api/connectors/templates/{id}` (solo il proprietario); nella pagina Connettori la sezione «Catalogo» con istruzioni e pulsante «Aggiungi»; parole come «email», «calendario», «evento» rendono pertinenti gli strumenti dei connettori.
- **Conseguenze:** collegare un servizio richiede solo la registrazione di un'app nella sua console (pochi minuti) e il consenso. Le API dei servizi possono cambiare: in particolare l'API v1 di Todoist va verificata con un account reale.
- **Evidenza:** unitario `every_template_is_a_valid_manifest` (tutti validi contro il contratto, ID client obbligatorio per OAuth); `rfc822_builds_a_utf8_message_and_refuses_header_injection`; `mail_is_sent_as_rfc822_only_after_confirmation` in `tests/connectors.rs`; E2E: Gmail senza ID client disabilitato, Google Calendar e GitHub aggiunti dal catalogo (79/79).
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ACCEPTANCE.md, ROADMAP.md, contratto `connector`.

### ADR-0048 — Competenze (Skills): procedure riutilizzabili del proprietario

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** richiesta del proprietario («aggiungi Skills e Competenze»).
- **Contesto:** le preferenze in memoria guidano lo stile, ma non descrivono procedure in più passi (briefing, revisione di testi, ricerca con fonti, registro delle spese). Il modello locale va guidato con istruzioni brevi e mirate, non con un prompt sempre più lungo.
- **Decisione:**
  - una competenza ha nome, descrizione, parole di attivazione, istruzioni (al massimo 4000 caratteri), strumenti suggeriti del catalogo e stato attivo/disattivo; è salvata nella tabella `skills` (schema 6);
  - sette competenze incluse al primo avvio, modificabili e disattivabili ma non eliminabili: «Briefing del mattino», «Scrittura e revisione», «Ricerca con fonti», «Organizzazione», «Spese», «Sicurezza difensiva», «Sviluppo e revisione del codice»;
  - una competenza si attiva quando la richiesta contiene una parola di attivazione o la richiama per nome («usa Spese: …», «competenza Spese»); al massimo 2 per richiesta;
  - il Core aggiunge le competenze attive agli input della delega (`inputs.skills`, aggiunta compatibile al contratto); il worker inserisce le istruzioni nel prompt di sistema, prima delle regole di sicurezza, e offre per primi i loro strumenti;
  - gli strumenti suggeriti devono esistere nel catalogo; possono allargare l'ambito dello specialista scelto per la richiesta (es. `todo.list` durante una ricerca con il briefing), ma non i permessi: policy, conferme e audit restano quelli di ogni strumento; gli agenti temporanei non ricevono competenze;
  - API solo del proprietario: `GET/POST /api/skills`, `PUT/DELETE /api/skills/{id}`; ogni modifica è registrata nell'audit (`skill.save`, `skill.delete`); nella chat il passo «competenza · <nome>» mostra quale procedura è stata applicata; nuova vista «Competenze» nella dashboard.
- **Alternative scartate:** competenze come plugin con codice (escluse dalla decisione 81 sui plugin isolati); attivazione decisa dal modello (meno prevedibile e più costosa in token con modelli piccoli).
- **Conseguenze:** il proprietario insegna a JARVIS nuove procedure senza modificare il codice. Le parole di attivazione sono confronti di testo semplici: parole troppo generiche attivano la competenza più spesso del voluto.
- **Evidenza:** `tests/skills.rs` (incluse, validazione, attivazione per parola e per nome, disattivazione, eliminazione, audit); `active_skills_enter_the_prompt_and_widen_only_the_offered_tools` in `tests/agent_loop.rs`; test Python del prompt e dell'ordine degli strumenti; E2E: competenza creata dalla dashboard e attivata in chat (84/84). Gli errori di lettura non vengono più convertiti in una lista vuota; l'ambito strumenti si chiude senza gli strumenti suggeriti e l'errore nel contesto della delega viene restituito. Il briefing predefinito viene aggiornato solo se descrizione, istruzioni e strumenti corrispondono ancora alla versione originale; le personalizzazioni del proprietario sono preservate.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ACCEPTANCE.md, ROADMAP.md, contratto `delegation`.

### ADR-0049 — Backend: bundle compresso, ambito degli strumenti calcolato una volta, conservazione dello storico

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** richiesta del proprietario («migliora il backend»).
- **Decisione:**
  - `app.js` e `app.css` vengono serviti con ETag (hash SHA-256 del contenuto), `Cache-Control: no-cache` e revalida con 304; con `Accept-Encoding: gzip` la versione compressa, calcolata una sola volta all'avvio (bundle da circa 320 KB a circa 100 KB, utile per telefoni e tablet in rete locale);
  - l'ambito degli strumenti di una delega (specialista, agente temporaneo, competenze attive) viene calcolato una volta per delega invece che per ciascuno dei 64 strumenti;
  - nella manutenzione periodica del database: per ogni automazione si conservano le ultime 200 esecuzioni; le cose da fare completate da più di 90 giorni vengono eliminate. Conversazione, attività, memoria e audit non cambiano conservazione.
- **Conseguenze:** meno traffico e avvio più rapido della dashboard dai dispositivi; il database non cresce senza limite con automazioni frequenti.
- **Evidenza:** `bundle_is_gzipped_and_revalidated_with_etag` in `tests/api.rs`; `history_keeps_the_latest_runs_and_recent_todos` in `store.rs`; suite Core completa (183 test).
- **Documenti interessati:** ROADMAP.md.

### ADR-0050 — Installer per Mac Intel e Linux ARM64

- **Data:** 2026-11-19 · **Stato:** accettato · **Fonte:** richiesta del proprietario («migliora le build e crea le release nuove»).
- **Decisione:** il workflow `release` aggiunge alla matrice `.dmg` per Mac con processore Intel (runner `macos-15-intel`) e `.deb` per Linux ARM64 (runner `ubuntu-22.04-arm`, es. Raspberry Pi 5), con le stesse prove degli altri installer (installazione reale e avvio del Core installato). Le due piattaforme sono facoltative (`continue-on-error`): se il runner non è disponibile per il repository, la release esce con le piattaforme principali; un limite di 90 minuti per job evita esecuzioni bloccate. Ogni file pubblicato riceve impronta, attestazione di provenienza e firma Sigstore come gli altri.
- **Conseguenze:** gli installer coprono Apple Silicon, Intel, Windows x64, Linux x64 e Linux ARM64. L'AppImage resta solo x64.
- **Evidenza:** esecuzione del workflow `release` per `v1.11.0`.
- **Documenti interessati:** README.md, note di rilascio, ROADMAP.md.

### ADR-0051 — Dashboard desktop e operazioni dei connettori

- **Data** 2026-11-19 · **Stato** implementato nel checkout · **Fonte** richiesta del proprietario.
- **Contesto** Tauri avvia il Core locale e ospita l’intera dashboard. La migrazione a SwiftUI richiede verifica di build e parità funzionale su macOS.
- **Decisione** La dashboard desktop usa superfici, tipografia di sistema e transizioni dedicate nella finestra Tauri. Le transizioni rispettano la preferenza di movimento ridotto.
- **Integrazioni** Il modulo mostra i parametri dallo schema pubblicato dal Core e invia un comando JSON in chat. L’agente lo riconosce senza una chiamata a Ollama. Il risultato strutturato è presentato localmente. Il Core valida, applica la policy e registra l’audit. Per OAuth il modulo presenta il collegamento al consenso e aggiorna lo stato al ritorno del proprietario.
- **Evidenza** Test di selezione nel gateway, test dei connettori nel Core ed E2E della nuova preparazione dalla dashboard.
- **Documenti interessati** README.md, AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0052 — Moduli dei connettori, chiarimenti e recupero dei dati

- **Data** 2026-11-20 · **Fonte** richiesta del proprietario.
- Il modulo delle operazioni genera campi da proprietà e tipi dello schema. Il Core applica lo schema completo. L’importazione di manifest per servizi personalizzati mantiene il suo formato.
- I parametri obbligatori assenti interrompono il turno con una domanda. Una scrittura il cui esito è incerto chiude il task come fallito e chiede di verificare la destinazione prima di ripetere.
- Il pannello Sistema verifica passphrase, impronte, SQLite e audit dei backup cifrati e mostra le copie prima degli aggiornamenti. Il Core e i comandi di recupero condividono un blocco esterno alla cartella dati. Il rollback verifica SQLite prima di installare la copia e conserva i dati precedenti.
- I nuovi backup e le copie prima degli aggiornamenti includono le preferenze e l’identità TLS locale. I pacchetti Python e i modelli scaricati restano installazioni locali.

### ADR-0053 — Avatar, postura difensiva, n8n e modelli cloud

- L’avatar offre controlli vocali e navigazione, con movimento ridotto rispettato. Il Centro sicurezza espone la postura locale, disponibile anche tramite lo strumento di sola lettura `security.status`.
- n8n locale usa Docker con rete disabilitata e volume dedicato. Il Core genera solo routine da modelli interni, separa l’importazione dall’esecuzione per evitare due scrittori SQLite e converte i segnali delle routine in notifiche locali.
- Ollama Cloud e NVIDIA NIM sono accessibili al proprietario per il singolo messaggio. Le chiavi usano l’archivio dei segreti. Il Core non invia memoria, cronologia o esiti di strumenti. NIM usa intervalli fra le richieste e un massimo di 35 RPM per Core, con attesa indicata dal provider dopo HTTP 429.

### ADR-0054 — Inferenza solo cloud e strumenti mediati dal Core

- **Data** 2026-11-20 · **Stato** implementato · **Fonte** richiesta del proprietario.
- Il Core usa il motore a regole per i comandi offline. Il setup installa la voce. Le precedenti scelte per Ollama locale e gli embedding vengono ignorate, mantenendo i ricordi e la loro ricerca locale. Questa decisione sostituisce le istruzioni relative ai modelli locali in ADR-0019, ADR-0028 e ADR-0045.
- Il proprietario può abilitare il passaggio da Ollama Cloud a NVIDIA NIM con «Cloud automatico». Il Core libera il proprio mutex durante la richiesta remota, distanzia le chiamate NIM a 35 RPM e registra nell’audit i provider tentati.
- Al provider arrivano il messaggio corrente e gli schemi degli strumenti selezionati dal catalogo. Il Core accetta una sola proposta e applica ambito, convalida, policy ed eventuale conferma. Il worker offline compone la risposta usando l’esito dello strumento locale. I connettori continuano a usare i moduli guidati e il motore a regole.
- **Evidenza** Test dei due formati API, del raffreddamento su 429 e del percorso proposta–policy–conferma nel Core. README e ROADMAP riportano la configurazione corrente.

### ADR-0055 — Continuità esplicita delle conversazioni cloud e riferimenti alle fonti

- **Data** 2026-11-20 · **Stato** implementato · **Fonte** richiesta del proprietario.
- «Continua la conversazione» autorizza l’invio di un massimo di tre scambi testuali consecutivi al provider che ha gestito quegli scambi. Il Core registra la provenienza delle nuove risposte testuali. I messaggi precedenti alla migrazione, i turni locali, i risultati degli strumenti, le conferme e gli errori restano fuori dal contesto cloud.
- «Cloud automatico» dà priorità al provider con contesto e, se passa all’altro, invia il messaggio corrente. L'istruzione del modello chiede di chiarire i riferimenti ambigui e separare fatti, ipotesi e fonti.
- Una richiesta esplicita come «Qual è la fonte?» legge solo le fonti archiviate con il turno precedente e viene risolta dal Core anche quando è selezionato un provider. La modalità offline chiede quale sia il riferimento di richieste vaghe e confronta parole intere per suggerire un ricordo.

### ADR-0056 — Tre installer desktop per Windows e macOS

- **Data** 2026-11-20 · **Fonte** richiesta del proprietario.
- La release pubblica contiene il setup NSIS Windows x64 e un’immagine macOS per ciascuna architettura (Apple Silicon e Intel). Tutti e tre i pacchetti sono necessari alla pubblicazione. La CI prova l’installazione con lo stesso setup o la stessa immagine che verrà pubblicata.
- La verifica prima della firma controlla la presenza dei tre file attesi. La configurazione di sviluppo e i test Linux rimangono disponibili. Le credenziali NIM si configurano per ciascuna installazione nell’archivio cifrato locale.

### ADR-0057 — Voce locale, sandbox Python e rilascio automatico

- **Data** 2026-11-20 · **Fonte** richiesta del proprietario.
- XTTS v2 usa checkpoint e audio di riferimento locali. Il worker carica i pesi al primo utilizzo, conserva il condizionamento e ripiega su Piper se il modello non è utilizzabile. La trascrizione usa faster-whisper con VAD, parole chiave tecniche e correzioni conservative. La chat richiede revisione manuale della dettatura destinata ai provider.
- Le istruzioni privilegiano verifiche di sviluppo riproducibili. Gli agenti temporanei di coding partono da strumenti di lettura e verifica della sintassi. L’esecuzione Python richiede conferma, Docker locale e un container effimero senza volumi o rete. Il terminale del PC è disabilitato per impostazione iniziale.
- Il Core limita contesto e risultati degli strumenti. Esclude dalla richiesta cloud le credenziali riconosciute e interrompe l’inclusione di turni precedenti che le contengono.
- Una CI `contracts` verde su `main` avvia il rilascio per una versione stabile senza tag. La build e il tag usano il commit verificato. I pacchetti seguono la matrice di distribuzione corrente e mantengono le verifiche di ADR-0056.

### ADR-0058 — Distribuzione desktop per Apple Silicon e Windows

- **Data** 2026-11-20 · **Fonte** richiesta del proprietario.
- La matrice del workflow `release` pubblica il `.dmg` Apple Silicon e il setup Windows x64. Entrambi devono superare la prova di avvio e il controllo delle impronte. La decisione aggiorna l’elenco di pacchetti di ADR-0056.

### ADR-0059 — Plancia vocale e colori dell’attività

- **Data** 2026-11-21 · **Fonte** richiesta del proprietario e immagine di riferimento.
- Un ritratto maschile originale nel bundle locale è al centro della Plancia. Il pulsante vocale usa il motore esistente, con stato, categoria e segnalazioni condivisi fra Plancia e Chat. La categoria iniziale deriva dal comando e viene aggiornata con agente e strumento restituiti dal Core.
- I colori accompagnano etichette testuali, con animazioni ridotte quando indicate dal sistema. Le viste usano le metriche già esposte dal Core.

### ADR-0060 — Controlli locali del PC dalla Chat

- **Data** 2026-11-21 · **Fonte** richiesta del proprietario.
- La Sidebar raggruppa le sezioni, mostra le conferme e conserva la preferenza di riduzione. La Chat riunisce avatar, conversazione, comandi vocali e instradamento.
- Il Core riconosce richieste di apertura esplicite, anche in Chat cloud, e autorizza solo destinazioni di sistema predefinite. La conferma monouso resta locale al computer del proprietario. Il Core applica i controlli anche ai client associati, alle deleghe e alla creazione di automazioni.

### ADR-0061 — Release dopo CI, routing cloud, catalogo e inviti

- **Data** 2026-11-21 · **Fonte** richiesta del proprietario.
- L’avanzamento di `main` avvia la CI anche per modifiche ai workflow e alla documentazione. Ogni esecuzione verde costruisce dallo SHA verificato e usa una versione numerica applicata alla build. I due installer e le attestazioni vengono pubblicati dopo i test. La retention che allora conservava le release precedenti è superata da ADR-0065.
- Il Core sceglie per entrambi i provider fra risposta rapida, analisi e sviluppo usando contenuto e lunghezza del contesto autorizzato. Mantiene il limite NIM a 35 RPM e prova una sola alternativa nello stesso provider su HTTP 404. La qualità è verificata con scenari sintetici; i pesi dei modelli ospitati restano gestiti dai provider.
- Il catalogo include Open-Meteo, GitLab e Airtable. Il modulo guidato crea manifest su campi tipizzati con la stessa validazione del Core. Una nuova installazione non può sovrascrivere un connettore esistente.
- L’esito finale di un tool fallito o negato risulta negli stati di attività. Il catalogo rispetta i vincoli delle automazioni e si aggiorna dopo nuove integrazioni.
- Gli inviti web usano un URL HTTPS di un host già autorizzato. Il codice è nel frammento, che il browser cancella dopo la lettura. Il nome e l’invio sono confermati sulla pagina di associazione. Il Core può sostituire o annullare un invito.

### ADR-0062 — Integrità della supply chain

- **Data** 2026-11-21 · **Stato** implementato · **Fonte** revisione di sicurezza.
- `ai/requirements-voice.in` contiene le sole dipendenze dirette. `ai/requirements-voice.txt` blocca anche le dipendenze transitive e gli hash SHA-256 dei pacchetti.
- L'avvio da sorgenti, il setup guidato e il workflow di release installano con `pip --require-hashes`; i pacchetti non corrispondenti al lock vengono rifiutati.
- Il lock è generato con Python 3.13 e pip-tools 7.7.0. I test CI di release verificano l'installazione su Windows e macOS.
- Le GitHub Actions di terze parti sono fissate a commit SHA; la CLI Tauri è installata alla versione esatta 2.12.6.

### ADR-0063 — Arresto ordinato dell'app desktop

- **Data** 2026-11-21 · **Stato** implementato · **Fonte** richiesta del proprietario e revisione del ciclo di vita desktop.
- Chiudere la finestra continua a nascondere JARVIS; la voce **Esci da JARVIS** del menu dell'icona di stato chiude l'app.
- Il launcher chiude lo stdin del Core, attende fino a otto secondi che il Core completi l'arresto e chiuda il worker, quindi forza la terminazione solo se il Core è ancora attivo; gli errori sono restituiti al chiamante per essere segnalati.
- **Evidenza:** test d'integrazione del launcher verificano l'uscita ordinata sia con il comando di arresto esplicito sia con la chiusura del processo padre.

### ADR-0064 — Leggibilità dell'icona alle dimensioni di sistema

- **Data** 2026-11-21 · **Stato** implementato · **Fonte** richiesta del proprietario.
- Si mantiene il simbolo del reattore e si riducono i dettagli minuti della corona; il nucleo e il contrasto restano riconoscibili alle dimensioni della tray di Windows e macOS.
- `scripts/make_brand_assets.py` resta la fonte riproducibile di icone e grafiche degli installer; i file generati sono versionati.

### ADR-0065 — Una sola release dopo CI verde su main

- **Data** 2026-11-21 · **Stato** implementato · **Fonte** richiesta del proprietario.
- **Contesto:** la retention precedente saltava le pubblicazioni automatiche (`inputs.automatico`) e lasciava crescere la pagina Releases.
- **Decisione:** ogni push su `main` che completa con successo il workflow `contracts` pubblica gli installer dallo SHA verificato. Dopo la pubblicazione, `scripts/prune_releases.py` mantiene la release SemVer più alta ed elimina le altre release e i rispettivi tag, incluse le prerelease. Una build vecchia che termina dopo una nuova non può rimuovere quella con versione maggiore.
- `gh release create` combina `--notes` e `--generate-notes` come documentato dalla CLI; il workflow rimuove il file temporaneo delle note anche in caso di errore.
- **Conseguenze:** la pagina Releases conserva un solo elemento e i tag delle release eliminate non sono più disponibili. Per recuperare una versione precedente si usa il rollback locale (ADR-0015) o si crea una nuova release. Il 2026-11-21 la cleanup è stata applicata anche alle release esistenti: è rimasta `v1.13.193`; le precedenti release e i relativi tag sono stati rimossi.
- **Evidenza:** test unitari SemVer, prerelease, retention e comandi di rimozione in `tests/contracts/test_release_cleanup.py`.

### ADR-0066 — Client iOS ritirato e push Android in background

- **Data** 2026-11-21 · **Stato** implementato in parte · **Fonte** richiesta del proprietario.
- Il client iOS non è più supportato: codice e progetto Xcode residui vengono rimossi; le note storiche restano solo come registro.
- Android usa notifiche FCM data-only ad alta priorità, con scadenza di 24 ore. Il client mostra solo un avviso generico, verifica il consenso locale e il permesso OS, e sospende il polling di chat/attività/notifiche quando l'app non è in primo piano.
- I quattro identificativi Firebase dell'app Android arrivano al build da variabili GitHub Actions; la credenziale service-account resta esclusivamente sul Core host, fuori dal repository e dai job di compilazione.
- `google-services.json` è solo una fonte locale ignorata da Git da cui importare questi identificativi nelle Actions variables. Il client OAuth `client_secret_*.json` non è usato da FCM o dai workflow; può essere necessario per autorizzare separatamente Gmail/Google Calendar.
- L'APK firmato `release` è pubblicato nella stessa release degli installer desktop; non viene pubblicato alcun APK `debug`.
- **Evidenze:** test sintetici del payload FCM e dei permessi Android; workflow release con variabili Firebase opzionali ma complete o assenti. La prova reale su dispositivo richiede le credenziali Firebase del proprietario.
- Il frontend Web è stato rimosso con ADR-0067; FCM Android usa il client nativo e non richiede un service worker.

### ADR-0067 — App desktop egui e ritiro dell'interfaccia Web

- **Data** 2026-11-21 · **Stato** implementato nel checkout, verifiche CI in corso · **Fonte** scelta esplicita del proprietario per la seconda PR.
- **Contesto:** rimuovere completamente il client Web/PWA e sostituire Tauri/WebView mantenendo i pacchetti macOS DMG e Windows setup EXE, il Core API e l'app Android.
- **Decisione:** il desktop usa Rust/egui con client HTTP autenticato e pairing HTTPS. Il Core continua a esporre API per i client nativi, ma non serve HTML, bundle, manifest o service worker. Le route API di pairing restano attive; i record device espongono solo `android`, `desktop` o `other`, e `/`, `/pair` e gli asset Web restituiscono 404.
- Il client aggiorna notifiche ogni cinque secondi mentre è attivo e usa le notifiche native di Linux, macOS e Windows quando non è in primo piano. Android conserva FCM data-only ad alta priorità secondo consenso e permesso OS.
- La CI sostituisce Chromium/Playwright con un test API offline (`tests/e2e/api_smoke.cjs`) e compila/testa il client desktop su Linux, macOS e Windows. Il workflow release produce DMG egui e installer NSIS. Il fixture E2E del token rispetta la validazione del Core (almeno 32 caratteri alfanumerici). I pacchetti pubblicati sono firmati con un certificato autofirmato privato su macOS e Windows, senza firma commerciale o notarizzazione.
- **Conseguenze:** la UI desktop non ha ancora parità con la dashboard rimossa: controlli vocali, moduli guidati completi per connettori/configurazione, viste tipizzate per alcuni pannelli e comportamento tray restano da implementare nei client nativi. La cronologia testuale della conversazione viene ripristinata all'apertura della chat. Queste lacune non autorizzano a reintrodurre il client Web.
- **Evidenze locali:** `cargo fmt --check`, Clippy e `cargo test --locked` del Core superati (un test Docker ignorato); test launcher 4/4; test `:core-client:test` Gradle superati; `tests/e2e/api_smoke.cjs` 15/15; `tests/smoke/start_smoke.py` 6/6 senza route Web; test Python e contratti superati (un test voce ignorato perché `openwakeword` non è installato). La prima CI ha individuato che il test di avvio leggeva ancora il token dalla pagina `/`; corretto per usare il token fixture passato dall'ambiente. Esito della CI aggiornata ancora atteso.
- **Documenti interessati:** README.md, ROADMAP.md, DEVELOPMENT.md, TECH_STACK.md, INTERFACE_AND_DEVICES.md, REQUIREMENTS.md e DEVOPS_QA_OBSERVABILITY.md.

### ADR-0068 — Firma desktop automatica con Secret e fallback temporaneo

- **Data** 2026-11-22 · **Stato** sostituita da ADR-0071 · **Fonte** richiesta del proprietario: usare i certificati configurati nei Secret senza passaggi manuali e non fallire se non sono disponibili.
- **Decisione:** DMG macOS e setup Windows usano prima i P12/PFX dei Secret con verifica della password e della chiave privata. Se mancano o non sono validi, il runner genera automaticamente un certificato autofirmato temporaneo e firma gli artefatti. Nessuna licenza Apple/Azure è necessaria.
- **Conseguenze:** macOS può richiedere l'autorizzazione manuale al primo avvio e Windows può mostrare avvisi SmartScreen. La pipeline non può essere bloccata da un Secret desktop assente o non valido. Le attestazioni OIDC/Sigstore restano obbligatorie e il keystore Android resta separato.
- **Documenti interessati:** DEVELOPMENT.md, GITHUB_ACTIONS_SECRETS.md, SECURITY_AND_PRIVACY.md, DEVOPS_QA_OBSERVABILITY.md e workflow `release.yml` / `release-after-ci.yml`.

### ADR-0069 — Credenziali OAuth Google locali per Gmail e Calendar

- **Data** 2026-11-21 · **Stato** implementazione locale; consenso Google del proprietario ancora da completare · **Fonte** scelta del proprietario per Gmail e Google Calendar.
- **Decisione:** lo script `scripts/configure_google_connectors.py` legge il client OAuth desktop locale, installa i manifest Gmail e Google Calendar con il suo ID client e invia il client secret soltanto al Core raggiunto su loopback. Il proprietario approva nel browser gli scope già dichiarati nei manifest; il Core conserva i token nel proprio archivio cifrato.
- Google Desktop richiede il redirect loopback `http://127.0.0.1:<porta>` senza percorso. La route `/` delega al callback OAuth solo se contiene `state`; altrimenti continua a restituire 404, senza ripristinare la pagina Web. Per gli endpoint Google il Core richiede anche `access_type=offline` e `prompt=consent`, così l'autorizzazione può fornire un refresh token per l'uso successivo. Lo script non autorizza automaticamente un account e non deve puntare a un Core remoto o non fidato.
- Il JSON OAuth non è un segreto Firebase né una variabile Actions. Dopo aver verificato entrambi i connettori autorizzati, il proprietario può eliminare il JSON client locale; prima di allora va mantenuto privato e ignorato da Git.
- **Evidenza:** test unitario `google_oauth_requests_a_refresh_token`; script di configurazione verificato sintatticamente. Il consenso reale e le chiamate ai servizi richiedono l'account Google del proprietario.
- **Documenti interessati:** FIREBASE_SETUP.md, `core/server/src/oauth.rs`, `scripts/configure_google_connectors.py`.

### ADR-0070 — Analisi del testo e del JSON nel Core

- **Data** 2026-11-22 · **Stato** implementata.
- Il Core conta parole separate da spazi, righe, caratteri Unicode e byte UTF-8. Il validatore JSON verifica la sintassi e riporta riga e colonna. Entrambi gli strumenti elaborano al massimo 12.000 caratteri e usano il permesso di calcolo già previsto dal catalogo.
- I comandi `conta le parole:` e `valida il JSON:` sono disponibili anche nella chat offline. Lo smoke test API verifica i due comandi, il conteggio Unicode e un errore di sintassi.

### ADR-0071 — Identità di firma desktop persistenti

- **Data** 2026-11-22 · **Stato** adottata. Sostituisce ADR-0068.
- Le release riutilizzano i certificati P12/PFX configurati nei Secret e richiedono una firma valida prima della pubblicazione. macOS firma app e DMG dal portachiavi temporaneo di CI. Windows firma setup, GUI e Core con il certificato identificato nel PFX.
- Le verifiche con i Secret configurati hanno installato entrambi i pacchetti e avviato il Core sui runner macOS Apple Silicon e Windows x64. Le attestazioni Sigstore e il keystore Android restano parte della pipeline.

### ADR-0072 — Configurazione guidata e verifica delle risposte

- Android legge messaggi e conferme in uno snapshot autenticato. Il client conserva la compatibilità con i Core che espongono le due rotte separate.
- Il client desktop usa il catalogo per installare connettori tramite un modulo. L’OAuth usa il browser e il setup voce usa le scelte e l’avanzamento del Core.
- Il proprietario sceglie la chat cloud e se includere il contesto recente dello stesso provider. I comandi deterministici riconosciuti, come la validazione JSON, si risolvono nel Core. Le richieste di sicurezza difensiva propongono strumenti di sola lettura.

### ADR-0073 — Plancia nativa con avatar animato

- **Data** 2026-11-22 · **Stato** implementata nel checkout · **Fonte** richiesta del proprietario e immagine di riferimento.
- **Contesto:** la Plancia egui era un riepilogo testuale, mentre Android si apriva sulla chat senza una home o un avatar. La UI Web precedente è stata ritirata con ADR-0067.
- **Opzioni considerate:** ripristinare immagini/WebView oppure disegnare l'avatar nei client nativi. Si sceglie la seconda per evitare dipendenze remote, mantenere la compatibilità Windows/macOS/Android e conservare i confini nativi.
- **Decisione:** la Plancia desktop usa una composizione a schede adattiva per Windows e macOS, con avatar vettoriale originale animato, accessi rapidi e riepiloghi provenienti dalle API del Core. Il controllo «Riduci animazioni» ferma il moto dell'avatar. Android aggiunge la Plancia come destinazione iniziale, con avatar Compose animato, accesso alla chat e attività recenti. Il supporto desktop Linux inizialmente previsto è stato ritirato dall'ADR-0075; Linux resta supportato per il Core headless.
- **Motivazione:** conteggi, stato e notifiche sono letti dal Core con le autorizzazioni già assegnate al dispositivo; non vengono inventate metriche hardware non disponibili. Intestazione e controlli di navigazione restano ancorati al viewport e il contenuto scorre nei pannelli interni. L'animazione Compose segue la scala di movimento del sistema.
- **Conseguenze:** la parità visiva con il riferimento è adattata ai client nativi; non si reintroducono immagini remote o il client Web. Il collaudo grafico su Windows, macOS e dispositivi Android reali resta necessario.
- **Evidenza:** `gradle -p apps/android -Pjarvis.coreOnly=true :core-client:test --no-daemon` e `git diff --check` superati. Il build dell'app Compose è sospeso perché nell'ambiente non è configurato l'Android SDK; Cargo e il collaudo visivo non sono disponibili.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md, `apps/desktop/src-tauri/src/app.rs`, `apps/android/app/src/main/java/local/jarvis/app/MainActivity.kt`.

### ADR-0074 — Rifinitura di memoria, notifiche e installazione dei client

- **Data** 2026-11-22 · **Stato** implementata nel checkout · **Fonte** richiesta del proprietario.
- **Contesto:** la ricerca Core non trovava alcuni plurali italiani quando la copertura lessicale usava forme non normalizzate; il client desktop non offriva ricerca nella vista Memoria; pairing, notifiche mobili e istruzioni di installazione avevano passaggi da rendere più chiari.
- **Decisione:** confrontare la copertura della memoria con gli stessi termini stemmatizzati della ricerca; aggiungere la ricerca testuale desktop tramite l'API già esistente; presentare categoria e gravità delle notifiche desktop, portare il push Android direttamente alla scheda notifiche e consentire la marcatura singola; includere istruzioni DMG e completare il wizard NSIS. Il pairing desktop consente di scegliere fra gli host HTTPS configurati e rende copiabili host selezionato, codice monouso e scope.
- **Permessi e privacy:** non cambiano scope, pairing o autorizzazioni. I dispositivi associati continuano a non accedere alla memoria del proprietario. FCM resta data-only e il testo del push rimane generico; gli installer non rimuovono i dati nella cartella utente.
- **Evidenza locale:** `gradle -p apps/android -Pjarvis.coreOnly=true :core-client:test --no-daemon`, `bash -n scripts/package_macos_release.sh`, parsing YAML del workflow release e `git diff --check` superati. Non eseguiti i test Rust e i build desktop perché Cargo/Rust non sono installati; il build Compose richiede l'Android SDK; prove installer e grafiche richiedono macOS/Windows. `tests/e2e/ui_smoke.cjs` non è presente nel checkout; i controlli installer aggiunti saranno eseguiti dalla CI release.
- **Documenti interessati:** README.md, INTERFACE_AND_DEVICES.md, ROADMAP.md, `core/server/src/memory.rs`, app desktop egui, app Android e flussi di release.

### ADR-0075 — Ritiro del desktop Linux

- **Data** 2026-11-22 · **Stato** adottata · **Fonte** scelta del proprietario in questa sessione.
- **Contesto:** la CI tentava di compilare il client egui e installare dipendenze X11 su Ubuntu, mentre la distribuzione desktop prevista è macOS e Windows.
- **Decisione:** il client desktop e i relativi installer sono supportati solo su macOS e Windows. Il launcher rifiuta `desktop` su Linux e il crate non è compilabile su sistemi non supportati; la CI desktop usa solo runner macOS/Windows. Il Core headless Linux, i job CI Ubuntu, gli archivi standalone del Core e i controlli API restano supportati.
- **Conseguenze:** le dipendenze X11 non sono più richieste dal client; la pipeline release continua a produrre DMG macOS, setup Windows e APK Android. Non si rimuovono i controlli Linux di Core, AI, API, audit o avvio.
- **Evidenza:** un run CI sul commit base ha mostrato errori `cargo fmt --check` nell'app desktop e nel test `memory.rs`; le correzioni di formattazione, la rimozione della matrice desktop Ubuntu e il test del rifiuto Linux sono nel checkout locale. La verifica completa e l'aggiornamento del lockfile sono registrati nel checkpoint roadmap.
- **Documenti interessati:** README.md, TECH_STACK.md, DEVELOPMENT.md, INTERFACE_AND_DEVICES.md, REQUIREMENTS.md, ACCEPTANCE.md, DEVOPS_QA_OBSERVABILITY.md, ROADMAP.md e workflow `contracts.yml`.

### ADR-0076 — Icona condivisa e avvio recuperabile

- **Data** 2026-11-22 · **Stato** correzione CI macOS su `main`; sorgente PNG definitiva nel checkout · **Fonte** richiesta del proprietario.
- **Contesto:** le risorse ICO/ICNS erano usate dagli installer ma non impostate come icona della finestra egui; un errore di connessione lasciava la schermata iniziale senza un'azione per riprovare.
- **Decisione:** conservare in `apps/desktop/src-tauri/icons/icon-source.png` il PNG sorgente definitivo fornito dal proprietario e derivarne le risorse statiche PNG, ICO, ICNS e Android adaptive icon. Ritagliare soltanto i margini trasparenti, conservare l'emblema originale e non sovrapporre riflessi o motivi che ne alterino la leggibilità. Applicare lo stesso marchio a finestra, schermata iniziale e installer, rispettando le safe zone e le dimensioni previste dalle piattaforme. La connessione resta asincrona; durante il caricamento si mostra lo stato locale/remoto e, in caso di errore, dettagli leggibili e un comando Riprova.
- **Conseguenze:** nessuna modifica alle API, ai permessi o al lifecycle del Core; installatori e client usano lo stesso marchio. Gli errori di avvio restano espliciti e non richiedono un riavvio manuale dell'app.
- **Evidenza:** la CI su `c2152c8` ha rilevato che `notify_rust::Notification::urgency` è disponibile su Windows ma non con il backend macOS configurato; la chiamata è ora condizionata a Windows. Le run complete `45035881025` e `45036515337` sono verdi per macOS, Windows, Android, Core e test trasversali. Il cross-build Windows, Clippy, la suite Core e i test Python locali sono superati; il controllo Android locale resta sospeso per assenza dell'SDK.
- **Documenti interessati:** `scripts/make_brand_assets.py`, risorse/app desktop e Android, ROADMAP.md.

### ADR-0077 — Plancia desktop e finestra DMG macOS

- **Data** 2026-11-22 · **Stato** implementata nel checkout; verifica visiva e CI macOS in attesa.
- **Contesto:** il tema egui seguiva il tema chiaro del sistema mentre la Plancia configurava solo i colori scuri; il riquadro di benvenuto poteva espandersi fino all'altezza della finestra e le schede non occupavano l'area centrale. Nel DMG mancavano impostazioni persistenti per disporre gli elementi nella finestra Finder.
- **Decisione:** forzare il tema scuro dell'app, estendere le schede principali alla larghezza disponibile e limitare l'altezza del riquadro iniziale desktop. Costruire il DMG in modalità modificabile, impostare in Finder la vista icone con JARVIS e Applications affiancati e le istruzioni visibili, quindi comprimere il volume. La CI macOS verifica la presenza del layout Finder insieme all'app, al collegamento Applications e alle istruzioni.
- **Conseguenze:** il desktop mantiene la navigazione e lo scorrimento interno già previsti; non cambiano API, autorizzazioni o contenuti dei dati. La disposizione dipende da Finder/macOS e va collaudata sul runner e su un Mac reale.
- **Evidenza locale:** `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check`, `bash -n scripts/package_macos_release.sh`, parsing YAML del workflow e `git diff --check` superati. Il cross-check Cargo macOS è bloccato dal compilatore C/SDK Apple assente nell'host Linux; il test visuale e la creazione effettiva del DMG sono sospesi al runner macOS.
- **Documenti interessati:** `INTERFACE_AND_DEVICES.md`, ROADMAP.md, app desktop egui e workflow `release.yml`.

### ADR-0078 — Gerarchia responsive della Plancia egui

- **Data** 2026-11-22 · **Stato** implementata nel checkout; verifica grafica su macOS/Windows in attesa.
- **Contesto:** la Plancia mostrava i sei stati come badge equivalenti e impilava attività e notifiche, senza una gerarchia visiva leggibile a colpo d'occhio.
- **Decisione:** presentare gli stati in schede con valore e colore semantico, distribuendole in tre, due o una colonna in base alla larghezza. Evidenziare Nuova chat come azione primaria e affiancare attività/notifiche solo quando c'è spazio sufficiente; nelle finestre strette le schede restano impilate. Disabilitare «Segna tutte come lette» quando non ci sono notifiche non lette.
- **Conseguenze:** i valori continuano a provenire dallo snapshot del Core; i colori non cambiano stati, permessi o azioni. La navigazione fissa e lo scorrimento interno non vengono modificati.
- **Evidenza locale:** `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check`, suite contratti (14 test), parsing YAML, sintassi shell e `git diff --check` superati. Il build dell'app e la resa su macOS/Windows non sono verificabili nell'host Linux: il desktop rifiuta Linux e manca il toolchain Apple; il test `tests/e2e/ui_smoke.cjs` non è presente.
- **Documenti interessati:** `INTERFACE_AND_DEVICES.md`, ROADMAP.md e `apps/desktop/src-tauri/src/app.rs`.

### ADR-0079 — Avatar vettoriale desktop rifinito e rimozione anteprime

- **Data** 2026-11-22 · **Stato** implementata nel checkout; verifica grafica nativa sospesa.
- **Decisione:** mantenere l'avatar desktop originale disegnato con egui, raffinandone silhouette, contorni, moduli auricolari, occhi, bocca e armatura delle spalle con forme vettoriali coerenti e motion control esistente. Non introdurre fotografie, asset remoti o nuove dipendenze. Eliminare le anteprime statiche create durante la sessione; conservare lo screenshot fornito dall'utente come riferimento.
- **Conseguenze:** identità e animazione restano native e scalabili su macOS/Windows. La resa finale va controllata su un sistema desktop supportato.
- **Evidenza locale:** fmt Rust, `cargo check --target x86_64-pc-windows-gnu --locked` e `git diff --check` superati. L'host Linux non dispone di display grafico per catturare la UI in esecuzione.
- **Documenti interessati:** `INTERFACE_AND_DEVICES.md`, ROADMAP.md e `apps/desktop/src-tauri/src/app.rs`.

### ADR-0080 — Avatar HUD con dettagli vettoriali e motion coerente

- **Data** 2026-11-22 · **Stato** implementata nel checkout.
- **Decisione:** arricchire l'avatar egui con alone a bassa intensità, quadrante con tacche gerarchiche, punti orbitali, crest e visiera, dettagli delle guance e collarino. Le luci pulsano con l'animazione esistente; «Riduci animazioni» continua a congelare anello e intensità.
- **Conseguenze:** avatar interamente vettoriale, scalabile e senza asset fotografici o rete. Il perimetro di disegno resta contenuto nel riquadro dell'avatar; intestazione, navigazione e scroll interno non cambiano.
- **Evidenza locale:** `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check`, `cargo check --target x86_64-pc-windows-gnu --locked` e `git diff --check` superati; la cattura runtime richiede un desktop Windows o macOS.
- **Documenti interessati:** `INTERFACE_AND_DEVICES.md`, ROADMAP.md e `apps/desktop/src-tauri/src/app.rs`.

### ADR-0081 — Finitura coerente dell'interfaccia desktop

- **Data** 2026-11-22 · **Stato** implementata nel checkout; collaudo visivo macOS/Windows sospeso.
- **Decisione:** uniformare i controlli egui con angoli, contorni, stati attivi e target cliccabili più leggibili; evidenziare la sezione selezionata con un indicatore laterale; sostituire l'emoji delle notifiche con un pulsante testuale stabile tra font di piattaforma e presentare l'origine Core in un indicatore cromatico locale/remoto.
- **Conseguenze:** stessa interfaccia e stessi flussi su Windows e macOS, senza modifiche a endpoint o credenziali. Intestazione e navigazione restano ancorate; lo scroll rimane nel pannello centrale.
- **Evidenza locale:** `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check`, cross-check Windows `cargo check --target x86_64-pc-windows-gnu --locked` e `git diff --check` superati. Non disponibile una cattura runtime multipiattaforma nell'host Linux.
- **Documenti interessati:** `INTERFACE_AND_DEVICES.md`, ROADMAP.md e `apps/desktop/src-tauri/src/app.rs`.

### ADR-0082 — Riepiloghi in sola lettura da più Core nella Plancia

- **Data** 2026-11-22 · **Stato** implementata nel checkout; build multipiattaforma e verifica grafica in attesa.
- **Contesto:** il client desktop supportava un solo Core per volta. Il proprietario ha scelto di aggregare più Core simultaneamente solo nella Plancia; Chat, dettagli e azioni devono restare sul Core attivo. L'indisponibilità di un Core non deve bloccare i risultati degli altri.
- **Decisione:** associare ciascun Core aggiuntivo tramite origine HTTPS e invito monouso, mantenendo credenziali separate nella configurazione locale con permessi restrittivi. Nella Plancia interrogare i Core aggiunti in parallelo con timeout breve e richieste di sola lettura a salute e attività; aggregare stati, conteggi e attività recenti, mostrando separatamente Core offline o parziali. Ridurre la risposta di salute ai campi necessari prima di inserirla nei riepiloghi. Le altre viste, la chat e le mutazioni continuano a usare esclusivamente il Core attivo. «Dimentica» rimuove la credenziale dal client, senza revocare il dispositivo remoto: la revoca è esplicita nella sezione Dispositivi del Core.
- **Motivazione:** offre una panoramica simultanea senza cambiare routing delle scritture, cronologia o proprietà dei dati, e isola i guasti di rete.
- **Conseguenze:** ogni Core resta autorità indipendente per dati, audit e autorizzazioni; il riepilogo usa solo ciò che la credenziale associata può leggere. Il conteggio delle notifiche è aggregato, ma l'elenco e le relative azioni restano sul Core attivo. I controlli visivi reali e la verifica multipiattaforma restano necessari.
- **Evidenza:** test unitari desktop per normalizzazione HTTPS, round-trip e permessi `0600` su Unix, aggregazione con Core offline e minimizzazione della risposta salute. Build e lint vengono registrati nel checkpoint roadmap.
- **Documenti interessati:** `README.md`, `INTERFACE_AND_DEVICES.md`, `ROADMAP.md` e app desktop egui.

### ADR-0083 — Ripresa cross-device tramite collegamenti verificabili

- **Data** 2026-11-22 · **Stato** implementata nel checkout; build completa dell'app Android in attesa dell'Android SDK.
- **Contesto:** desktop e Android condividono già cronologia, attività e notifiche quando usano lo stesso Core, ma una notifica Android apriva solo l'elenco generico e l'obiettivo di un task non aveva un'azione di ripresa.
- **Decisione:** passare al tap push l'ID della notifica (non titolo, corpo o ref), aprire la relativa notifica e metterla in evidenza nell'elenco ottenuto dal Core autenticato. Le notifiche API conservano un `link` validato secondo le viste del contratto; Android offre navigazione diretta per chat, attività e Plancia. I link task selezionano ed evidenziano l'attività se è nell'elenco recente. La Plancia desktop e Android offrono **Prepara ripresa in chat**, che mette l'obiettivo del task nella bozza della chat condivisa senza inviare il messaggio né eseguire azioni.
- **Motivazione:** migliora continuità fra dispositivi usando le API e i permessi esistenti, con una scelta esplicita dell'utente prima di inviare o produrre effetti.
- **Conseguenze:** il Core resta la fonte di verità e la sola destinazione della chat; il payload FCM rimane privo di contenuto privato e riferimenti task. Le viste non disponibili sul client mobile non vengono simulate.
- **Evidenza:** test `CoreClient` per parsing del link supportato e rifiuto di viste non riconosciute; suite del modulo superata. Il compile Kotlin dell'app è sospeso perché manca Android SDK nell'host. La correzione CI macOS importa `PermissionsExt` nello scope del secondo blocco Unix, evitando l'errore `E0599`; fmt e check/lint Windows superati, ma la conferma macOS richiede CI.
- **Documenti interessati:** `README.md`, `INTERFACE_AND_DEVICES.md`, `ROADMAP.md`, client Android e desktop.

### ADR-0084 — Revisione proattiva e non mutante delle scadenze

- **Data:** 2026-11-22 · **Stato:** implementata nel checkout.
- `todo.review` legge attività aperte scadute o in scadenza entro un orizzonte predefinito di 24 ore, configurabile fra 1 ora e 7 giorni. Restituisce gruppi distinti e un suggerimento per ciascuna attività; non completa, rischedula o crea attività.
- La competenza Briefing e Organizzazione possono usare lo strumento. I comandi offline comprendono «controlla le scadenze» e «cosa devo fare oggi». Le notifiche di scadenza esistenti non vengono duplicate né anticipate; non vengono inviati messaggi o eseguite azioni automaticamente.
- **Evidenza:** test Core dalla chat con attività sintetiche scadute e imminenti, controllo che restino aperte dopo la revisione, test AI per instradamento offline e descrizione dei suggerimenti.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, README.md, ROADMAP.md e `contracts/catalog/tools/todo.review.json`.

### ADR-0096 — Missioni autonome

- **Data:** 2026-11-23 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora la proattività di JARVIS e come agente autonomo come nel film»).
- **Contesto:** fino a qui JARVIS agiva solo se interpellato, un passo alla volta, e l'inferenza cloud era limitata a quattro strumenti di calcolo. Non esisteva un modo per affidargli un obiettivo da perseguire da solo.
- **Decisione:**
  - una **missione** ha un obiettivo (8–1000 caratteri, senza credenziali), una ricorrenza facoltativa (da 5 minuti a 7 giorni), un limite di esecuzioni (predefinito 3, o 100 se ricorrente), il consenso esplicito all'uso dei connettori e un diario delle ultime 10 esecuzioni; al massimo 8 missioni aperte (tabella `missions`, schema 9);
  - lo scheduler avvia al massimo una missione per giro. L'esecuzione prenota la missione per 30 minuti, così un arresto a metà non la duplica. Con una chiave cloud il modello guida l'agente passo per passo (`cloud::AgentRequest`, fino a 5 strumenti e poi il rapporto): ogni richiesta al provider avviene fuori dal blocco del Core, come le chat, quindi chat e automazioni non attendono. Senza chiave la missione usa il worker locale a regole;
  - ogni strumento rientra nel Core ed è verificato da ambito concesso, policy, blocco delle ripetizioni e audit, con identità `agent:mission`. Gli strumenti concessi sono solo quelli senza conferma, a rischio non alto, in sola lettura più `notes.add` e `todo.add`; esclusi `agent.delegate`, `mission.*`, l'apertura di applicazioni e, salvo consenso dell'obiettivo, i connettori e i nodi. Un'azione che richiederebbe conferma viene descritta nel rapporto, mai tentata;
  - il rapporto inizia con «COMPLETATA:» o «IN CORSO:». Una missione non ricorrente si chiude al primo rapporto senza «IN CORSO:»; una ricorrente si ripete fino al limite. Tre errori consecutivi la interrompono; ogni esecuzione produce una notifica `mission`. Se il proprietario mette in pausa o annulla durante un'esecuzione, la scelta prevale;
  - i risultati degli strumenti inviati al modello sono ridotti e, se contengono credenziali, sostituiti da un avviso;
  - API solo del proprietario: `GET/POST /api/missions`, `POST /api/missions/{id}/{pause|resume|cancel|delete|run}`. Dalla chat: `mission.start` (con conferma), `mission.list`, `mission.cancel`; offline «avvia una missione [ogni N minuti]: …».
- **Alternative scartate:** missioni che usano tutti gli strumenti con conferma in coda (un'azione approvata ore dopo è difficile da giudicare e allarga la superficie di prompt injection); esecuzione sotto il blocco del Core (bloccherebbe la chat per tutta la durata delle chiamate al modello).
- **Conseguenze:** JARVIS può lavorare in autonomia su ricerche, controlli e riepiloghi con un perimetro di strumenti ristretto e verificabile. Una missione cloud invia obiettivo e risultati degli strumenti al provider scelto: è una scelta esplicita del proprietario, come per la chat. L'esito dipende dalla qualità del modello e non è garantito.
- **Evidenza:** `tests/missions.rs` (7 test: esecuzione locale, ricorrenza, pausa/annullamento anche durante l'esecuzione, strumenti con conferma esclusi, avvio dalla chat con conferma, limite di missioni, ciclo guidato dal cloud con strumento eseguito, rifiuto fuori ambito, completamento ed errore del provider); test unitari di `missions.rs` e di `cloud::AgentRequest` (formato dei messaggi Ollama e NIM, scambio HTTP sintetico); smoke API; prova con il Core reale: lo scheduler ha eseguito e completato una missione da solo.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ACCEPTANCE.md, ROADMAP.md, contratti `delegation` (`steps_left` fino a 8), `notification` (categorie `mission`, `briefing`, `suggestion`) e strumenti `mission.*`.

### ADR-0097 — Iniziative proattive

- **Data:** 2026-11-23 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario.
- **Contesto:** le notifiche esistenti reagiscono a un evento (scadenza, esito di un'automazione). Mancava un assistente che si faccia avanti con ciò che serve sapere.
- **Decisione:** il pianificatore valuta ogni minuto regole deterministiche e locali, ciascuna attivabile e con una sola emissione per chiave (tabella `proactive_sent`, pulita dopo 30 giorni):
  - **briefing** quotidiano all'orario scelto (predefinito 08:00) e solo nelle 6 ore seguenti: cose da fare aperte, in scadenza oggi e in ritardo, conferme in attesa, notifiche non lette, missioni attive, automazioni con errori e la priorità consigliata;
  - **spazio disco** oltre il 90% su volumi di almeno 1 GB, una volta al giorno per volume;
  - **conferme dimenticate** da più di 15 minuti, una volta per conferma;
  - **automazioni** con le ultime 3 esecuzioni fallite, una volta al giorno;
  - **cose da fare lasciate indietro**: almeno 3 aperte da oltre 7 giorni, una volta alla settimana;
  - **ore di silenzio** (predefinite 22:00–07:00): nulla viene inviato e le regole riprovano al giro successivo.
  Impostazioni in `proactive.json` (validate, con fallback ai predefiniti), `GET/PATCH /api/proactive` e `POST /api/proactive/briefing` per un briefing su richiesta. Nuove categorie di notifica `briefing` e `suggestion`.
- **Privacy:** nessuna regola usa il cloud o la rete; i dati restano nel Core.
- **Conseguenze:** niente lettura del calendario nel briefing: i connettori restano a richiesta e non vengono interrogati senza che l'obiettivo lo consenta (ricorrono alle missioni con consenso ai connettori). Nessun apprendimento delle abitudini: le regole sono fisse.
- **Evidenza:** `tests/proactive.rs` (briefing una volta al giorno con lo stato reale, finestra oraria, ore di silenzio e impostazioni, conferma ricordata una sola volta, briefing su richiesta); test unitari di `proactive.rs` (finestra, silenzio a cavallo della mezzanotte, validazione e persistenza, testo, soglie del disco); smoke API.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ACCEPTANCE.md, ROADMAP.md, contratto `notification`.

### ADR-0098 — Notifiche in tempo reale e push FCM robusto

- **Data:** 2026-11-23 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora il database Firebase e le notifiche in tempo reale»).
- **Contesto:** nel progetto Firebase è usato solo FCM per le push Android; non esiste un database Firebase e i dati restano in SQLite nel Core (scelta local-first, il proprietario ha confermato di migliorare FCM e il tempo reale senza nuovi servizi cloud). Le notifiche arrivavano ai client con un polling ogni 5-15 secondi; l'invio FCM non rimuoveva i token morti, non riprovava dopo un errore e firmava un nuovo token OAuth a ogni notifica.
- **Decisione:**
  - **Tempo reale:** `GET /api/notifications/stream` (Server-Sent Events, stessa autenticazione e stessi permessi di `GET /api/notifications`). Ogni notifica creata dal Core è inoltrata subito ai client collegati; un segnale ogni 20 secondi tiene viva la connessione e un evento `resync` dice al client in ritardo di rileggere l'elenco. Al massimo 16 flussi contemporanei. Desktop (egui), Android e macOS ascoltano il flusso e rileggono i dati all'arrivo; il polling resta come rete di sicurezza e rallenta (30-60 secondi) mentre il flusso è attivo. In background restano le push.
  - **FCM:** i token che FCM dichiara non più validi (404, o 400 `UNREGISTERED`) sono segnalati e il Core li elimina, con il loro segreto cifrato, al prossimo invio; errori temporanei (429, 5xx, rete) sono ritentati fino a 3 volte con attesa esponenziale o `Retry-After` (massimo 30 secondi); una credenziale rifiutata (401/403) rinnova il token OAuth e riprova una volta; il token OAuth è riusato per 50 minuti; ogni principal conserva al massimo 5 registrazioni (le più vecchie sono sostituite); priorità alta e scadenza di 24 ore solo per gli avvisi che richiedono attenzione, priorità normale e 6 ore per il resto, con `collapse_key` per briefing e suggerimenti. Il contenuto privato non viaggia mai: restano il testo generico e l'identificativo.
  - `GET /api/push/status` (solo proprietario): configurazione, registrazioni valide e da eliminare, ultimo invio riuscito, mai i token.
- **Alternative scartate:** Firestore o Realtime Database per sincronizzare messaggi e notifiche (sposta dati privati su un servizio cloud, contro il principio local-first); WebSocket (più complesso di SSE per un flusso a una direzione e meno adatto ai proxy HTTPS).
- **Conseguenze:** le notifiche arrivano in pochi millisecondi con l'app aperta; il registro dei token non cresce con app disinstallate. Un push non è comunque garantito: dipende da Android, dal produttore e dalle credenziali del proprietario.
- **Evidenza:** test unitari di classificazione delle risposte FCM, priorità e scadenza, tetto delle registrazioni, migrazione e pulizia dei token; `notifications_arrive_live_on_the_event_stream` (flusso autenticato, evento in tempo reale, stato push senza token); test del parser SSE sul desktop e su Android (`CoreClientTest`). L'invio reale a un telefono richiede un progetto Firebase del proprietario.
- **Documenti interessati:** FIREBASE_SETUP.md, INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0099 — Indurimento del backend

- **Data:** 2026-11-23 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («più robusto e senza falle o criticità»).
- **Revisione:** letti il filtro di accesso, il pairing, le rotte nuove e i punti di ingresso; verificata l'assenza di `unwrap` su input esterno nel codice di produzione.
- **Problemi trovati e correzioni:**
  1. il token del proprietario era confrontato con `!=`, cioè con uscita al primo byte diverso: ora il confronto è a tempo costante;
  2. ogni richiesta remota con un token inventato prendeva il blocco del Core per cercarlo nel database, quindi una pioggia di richieste da Internet poteva togliere la parola al proprietario: i token che non sono 64 caratteri esadecimali sono scartati subito e, dopo 30 tentativi falliti in 60 secondi, le richieste remote con token ben formato ma sconosciuto ricevono 429 senza toccare il Core (il canale locale del proprietario non è coinvolto; durante un attacco anche i dispositivi legittimi attendono, al massimo un minuto);
  3. i flussi in tempo reale potevano essere aperti senza limite: massimo 16;
  4. una cartella dati creata da una versione precedente poteva essere leggibile da altri utenti del computer: all'avvio viene riportata a `0700` su Unix.
- **Verificato senza modifiche:** host ammessi contro il DNS rebinding, intestazioni di sicurezza e `no-store` su ogni risposta, limite del corpo delle richieste, rotte nuove (`/api/missions`, `/api/proactive`, `/api/push/status`, competenze, memoria) negate ai dispositivi associati, scritture atomiche con permessi `0600`, token FCM cifrati.
- **Limiti residui:** nessun limite di tempo sulla lettura delle intestazioni HTTP (il Core ascolta solo su loopback e i dispositivi arrivano da un proxy che lo applica); un attacco alla macchina da un altro utente con gli stessi permessi del proprietario resta fuori dal modello di minaccia. Un audit indipendente non è stato eseguito.
- **Evidenza:** `tests/hardening.rs` (permessi dei dispositivi sulle rotte nuove, token malformati e sconosciuti con il limite, token esatto del proprietario, limite dei flussi).
- **Documenti interessati:** SECURITY_AND_PRIVACY.md, ROADMAP.md.

### ADR-0100 — Blocco e identità sull'app macOS

- **Data:** 2026-11-23 · **Stato:** implementata nel checkout; non compilata in questo ambiente (nessun Swift), verifica demandata alla CI macOS · **Fonte:** richiesta del proprietario («migliora il sistema di accesso sul Mac»).
- **Contesto:** l'app chiedeva Touch ID o la password solo all'avvio; dopo non si bloccava mai e le operazioni sensibili non richiedevano di nuovo l'identità.
- **Decisione:**
  - **Blocco dell'interfaccia** dopo 5 minuti senza tastiera, mouse o scorrimento, quando il Mac va in stop, quando lo schermo si blocca. Da bloccata l'app non mostra contenuti, cancella i dati caricati, chiavi e campi sensibili dalla memoria dell'interfaccia e rifiuta qualsiasi richiesta al Core; si sblocca con Touch ID o la password del Mac.
  - **Il Core continua a lavorare** mentre l'interfaccia è bloccata, così missioni, automazioni, notifiche e dispositivi associati non si interrompono. Conseguenza dichiarata: il token di sessione resta nella memoria del processo, che è anche quello che avvia il Core. Fermare il Core a ogni blocco avrebbe disconnesso telefoni e missioni.
  - **Identità per le azioni sensibili**: dispositivi (inviti e revoche), chiave cloud, connettori, backup, passkey, aggiornamenti e canali di notifica richiedono Touch ID o la password, valida un minuto dopo l'ultima conferma.
  - schermata di accesso rifinita (icona con alone, messaggio che distingue blocco e primo accesso) e sezione **Missioni** con le iniziative proattive.
- **Alternative scartate:** fermare il Core a ogni blocco (perde missioni e dispositivi); conservare le credenziali nel Portachiavi protetto da Touch ID (rimandato: non è stato scelto dal proprietario).
- **Evidenza:** solo revisione del codice; nessun compilatore Swift né macOS disponibile. Da collaudare su un Mac: blocco per inattività, per stop e per schermo bloccato, ripresa con Touch ID e con la password, richiesta d'identità alla revoca di un dispositivo.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, SECURITY_AND_PRIVACY.md, ROADMAP.md.

### ADR-0101 — Rifiniture grafiche: Android in stile iOS, Windows e macOS

- **Data:** 2026-11-23 · **Stato:** implementata nel checkout; resa visiva non verificata (nessuna cattura grafica né SDK Android in questo ambiente) · **Fonte:** richiesta del proprietario.
- **Android in stile iOS:** palette di sistema di iOS (sfondo raggruppato grigio chiaro o nero, schede bianche o grigio scuro, blu di sistema, separatori sottili), gerarchia tipografica di iOS (titolo grande 34, titoli 28/22, headline 17, corpo 17, nota 13, didascalia 10), angoli più generosi, barra a schede senza indicatore a pillola con etichette da 10 sp in blu o grigio, fumetti come in iMessage (blu per l'utente, superficie neutra per JARVIS, angolo a coda) e interruttori con traccia verde. L'identità di JARVIS resta nell'avatar.
- **Windows (egui):** testo più leggibile (corpo 15, titolo 24), finestre e menu con angoli più morbidi, campi di testo più scuri della superficie, bordi discreti, link in tinta.
- **macOS (SwiftUI):** schermata di accesso e sezione Missioni (vedi ADR-0100).
- **Conseguenze:** l'aspetto cambia senza modificare flussi o permessi; i valori sono nei file di tema e si regolano in un punto solo. Va verificato su un telefono, su Windows e su un Mac.
- **Evidenza:** il client Android compila solo in CI (manca l'SDK qui); `cargo clippy` del client egui pulito con le parti specifiche delle piattaforme sostituite temporaneamente.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md, `JarvisTheme.kt`, `app.rs`, `JarvisApp.swift`.

### ADR-0102 — Note di rilascio con APK e verifica

- **Data:** 2026-11-23 · **Stato:** implementata nel checkout.
- **Decisione:** le note della release elencano anche l'APK Android, indicano la versione minima di macOS (14, come nel pacchetto Swift) e contengono i comandi per verificare impronta, attestazione GitHub e firma Sigstore; le modifiche della versione restano generate da GitHub.
- **Documenti interessati:** `.github/release-notes.md`.

### ADR-0103 — Arresto del Core senza processi rimasti

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («controlla l'esito della release e trova altri errori»).
- **Problema:** la release automatica dopo la PR #76 è fallita nella prova «Installazione dal .dmg e avvio del Core installato» (macOS): il Core ha stampato «JARVIS arrestato» ma era ancora vivo 15 secondi dopo la chiusura dello stdin da parte del padre, e il worker Python è rimasto orfano. L'esecuzione successiva è passata: il difetto è intermittente. Causa probabile: al termine di `main` il runtime Tokio attende i compiti bloccanti ancora in corso (una richiesta lenta, una lettura di rete) e il processo non esce.
- **Decisione:** dopo l'arresto del server il Core prende il blocco (al massimo 3 secondi), salva l'ultima istantanea delle metriche, ferma il worker AI e porta il WAL del database nel file principale (`Jarvis::shutdown`), poi esce esplicitamente con codice 0. Vale per Ctrl+C e per la chiusura dello stdin.
- **Conseguenze:** l'uscita avviene in pochi centesimi di secondo (misurata: 20-30 ms) e non lascia il worker Python. Una richiesta ancora in corso viene interrotta: per l'arresto richiesto dal proprietario o dal padre è il comportamento voluto.
- **Altro corretto nella stessa revisione:** il client egui apriva un nuovo flusso di notifiche a ogni riconnessione al Core senza chiudere i vecchi; con il limite di 16 flussi del Core si sarebbero esauriti. I thread di una connessione sostituita ora si fermano da soli (contatore di generazione, controllato a ogni riga del flusso, segnali di vita compresi).
- **Verificato senza difetti:** 1702 richieste malformate (corpi vuoti, `null`, JSON non valido, tipi sbagliati, metodi non previsti) su tutte le rotte del Core: nessuna risposta 5xx, nessun blocco, nessun panic, il Core è rimasto vivo.
- **Evidenza:** `tests/shutdown.rs` (avvia il binario, serve una chat che fa partire il worker, chiude lo stdin e richiede uscita con codice 0 entro 10 secondi); test del parser SSE con flusso superato; suite del Core stabile in due esecuzioni consecutive (276 test).
- **Documenti interessati:** ROADMAP.md.

### ADR-0104 — Configurazione Firebase verificabile e credenziale FCM nella cartella dati

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («aiutami a configurare Firebase»).
- **Problema:** la credenziale dell'account di servizio era letta solo da `JARVIS_FCM_SERVICE_ACCOUNT_FILE`, una variabile che le app installate (aperte dal Finder o dal menu Start) non ereditano dalla shell; inoltre l'APK della release `v1.13.290` non conteneva gli identificativi Firebase, e la build non lo segnalava (il controllo accettava sia «tutte e quattro le variabili» sia «nessuna»).
- **Decisione:** il Core cerca la chiave prima nella variabile e poi in `<cartella dati>/fcm-service-account.json`, rileggendola a ogni invio (nessun riavvio). `scripts/check_firebase.py` verifica, senza stampare segreti, `google-services.json` (pacchetto `local.jarvis.app`), il file dell'account di servizio (tipo, campi, permessi, stesso progetto dell'app), la presenza degli identificativi in un APK e lo stato del Core (`GET /api/push/status`). La guida in FIREBASE_SETUP.md indica l'ordine: variabili e nuova release, chiave sul Core, attivazione sul telefono, prova con «Briefing adesso».
- **Non verificato:** l'invio reale a un telefono (richiede il progetto Firebase del proprietario); il contenuto delle variabili del repository, che da questa sessione non è leggibile.
- **Evidenza:** `test_firebase_check.py` (configurazione coerente, pacchetto sbagliato, progetti diversi, chiave non di servizio, APK con e senza identificativi, nessun segreto nell'output); test del Core sul percorso nella cartella dati.
- **Documenti interessati:** FIREBASE_SETUP.md, ROADMAP.md.

### ADR-0105 — Terminale sandbox isolato per codice e comandi

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («un suo terminale Sandbox per eseguire codice e comandi»).
- **Contesto:** `shell.run` esegue sul computer del proprietario e chiede conferma a ogni comando; `code.run_python` è un container monouso solo Python. Mancava un luogo dove JARVIS potesse provare codice e comandi in autonomia, con file che sopravvivono fra una chiamata e l'altra.
- **Decisione:** tre strumenti, `sandbox.run` (`sh -c` in un container Docker; sessione `^[a-z0-9-]{1,32}$` persistente, timeout 1–60 s, output 16 KiB), `sandbox.status` (sola lettura: disponibilità, limiti, sessioni) e `sandbox.reset` (azzera una sessione, con conferma). Il container non ha rete, radice in sola lettura, nessuna capability, `no-new-privileges`, 512 MB di memoria, 1 CPU, 128 processi, `/tmp` in tmpfs da 64 MB e un solo mount: `<cartella dati>/sandbox/<sessione>` su `/work` (0700; utente uguale al proprietario della cartella su Unix, 65534 su Windows). Nessun mount della workspace, dei segreti o del socket Docker. Una sessione oltre 256 MiB rifiuta nuovi comandi, oltre 1 GiB viene azzerata. Immagine: `JARVIS_SANDBOX_IMAGE`, poi `jarvis-sandbox:latest` (`infra/sandbox/build.sh`), poi `python:3.13-alpine`; mai scaricata a runtime (`--pull never`).
- **Consenso:** `sandbox.run` non chiede conferma per comando (rischio medio, effetti `write_local`) perché non può toccare il computer né la rete; resta tracciato in audit e nel diario del task. Il proprietario può spegnerlo con `PATCH /api/sandbox {"enabled": false}` (solo dal computer del Core, mai da un dispositivo) o con `JARVIS_SANDBOX=off`; spento, ogni chiamata risponde `sandbox.disabled`. `sandbox.run` e `sandbox.reset` sono esclusi da automazioni e nodi; `sandbox.run` è consentito alle missioni (senza conferma, non rischio alto) e allo specialista di codice; `sandbox.reset` no.
- **Regole offline:** «esegui nella sandbox: …», «sandbox: …», «stato sandbox», «azzera la sandbox [sessione]». Le chat cloud restano limitate ai quattro strumenti di calcolo (ADR-0054).
- **Limiti:** serve Docker; nessuna rete nella v1 (le dipendenze vanno incluse nell'immagine); `shell.run` sul computer resta con conferma e disattivato per impostazione predefinita.
- **Non verificato:** l'esecuzione reale nel container (nessun Docker in questo ambiente): c'è un test `#[ignore]` da lanciare con Docker.
- **Evidenza:** test unitari (nomi di sessione, argomenti di isolamento, interruttore, validazione prima di Docker), `core/server/tests/sandbox.rs`, test del gateway, schema dei descrittori.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, SECURITY_AND_PRIVACY.md, ROADMAP.md, `infra/sandbox/`.

### ADR-0106 — Trasferimento file fra workspace e sandbox

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («altre ADR»), seguito di ADR-0105.
- **Problema:** il terminale sandbox non vede la workspace: JARVIS non poteva provare un file del progetto né riportare un risultato.
- **Decisione:** due strumenti. `sandbox.import {from, to?, session?}` copia un file della workspace (max 8 MiB) nella sessione, senza conferma perché legge soltanto. `sandbox.export {from, to, session?}` copia un file della sessione (max 1 MiB) nella workspace e **richiede conferma** perché scrive sul computer. Solo file singoli; percorsi relativi senza «..».
- **Sicurezza:** il codice nel container può creare collegamenti simbolici che sul computer punterebbero altrove. Ogni percorso nella sessione è risolto componente per componente rifiutando qualunque collegamento già presente (`sandbox.invalid_path`); la destinazione in workspace usa la stessa risoluzione confinata di `fs.write`. Rispettano l'interruttore del proprietario; `sandbox.export` è escluso da automazioni e nodi; `sandbox.import` è consentito alle missioni.
- **Regole offline:** «importa nella sandbox: file [come percorso]», «esporta dalla sandbox: origine in destinazione».
- **Evidenza:** test unitario con collegamenti verso file e cartelle esterni (nessuna perdita, nessuna scrittura fuori ambito), test dei descrittori (consenso), test del gateway.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, SECURITY_AND_PRIVACY.md, ROADMAP.md.

### ADR-0107 — Avviso proattivo di sandbox quasi piena

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout.
- **Decisione:** una nuova iniziativa proattiva (`sandbox`, attiva per impostazione predefinita, spegnibile in Proattività) avvisa una volta al giorno per ogni sessione sandbox oltre 200 MiB, prima del limite di 256 MiB oltre il quale i comandi vengono rifiutati. Non parte se il terminale sandbox è disattivato; rispetta ore di silenzio e `proactive_sent`. Il testo propone di esportare i file utili e azzerare la sessione, senza farlo da sola.
- **Evidenza:** test della soglia (`sandbox_alerts`), impostazione predefinita, build egui.
- **Non verificato:** resa dell'interruttore nella schermata Swift (compilata solo in CI).
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0108 — Rete HTTPS affidabile verso modelli e servizi

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («molti problemi nel collegamento HTTPS per i modelli AI»).
- **Causa individuata:** `ureq` con la sola funzione `native-certs` si fida **soltanto** dell'archivio di certificati del sistema, senza ripiego: con un archivio incompleto o illeggibile (Windows che installa le radici su richiesta, Linux senza `ca-certificates`) ogni connessione falliva, e l'unico messaggio era «Connessione HTTPS non riuscita». Inoltre il tempo massimo di 45 s interrompeva i modelli di ragionamento e veniva descritto come errore di connessione, e un errore DNS o di connessione momentaneo non veniva ritentato.
- **Decisione:** nuovo modulo `net.rs`: configurazione TLS condivisa con radici del sistema **più** radici incluse (`webpki-roots`), provider `ring`; agente base con connessione a 10 s, nessun reindirizzamento e User-Agent esplicito. Usato da modelli, notifiche push, canali, aggiornamenti e Web. Per i modelli: tempo massimo 90 s, fino a due nuovi tentativi (0,4 s e 1,2 s) solo per cause transitorie (DNS, connessione rifiutata o interrotta), mai dopo un timeout in lettura.
- **Diagnosi:** `Failure::Connection(Cause)` distingue DNS, connessione rifiutata, timeout, interruzione, certificato, proxy; il messaggio dice la causa e la correzione più probabile (data e ora del computer, antivirus che ispezionano il traffico, `HTTPS_PROXY`). Certificato e proxy non vengono proposti come ritentabili.
- **Limiti:** un proxy aziendale che ispeziona il traffico richiede che la sua CA sia nell'archivio di sistema; le app aperte da Finder o dal menu Start non ereditano `HTTPS_PROXY` dalla shell.
- **Evidenza:** test della classificazione, delle radici incluse, di una connessione rifiutata e dei due tentativi con attesa.
- **Documenti interessati:** ROADMAP.md.

### ADR-0109 — Aggiornamento dall'interno dell'app

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («gli aggiornamenti direttamente all'interno»). Supera la regola «nessuna installazione automatica» di ADR-0034.
- **Decisione:** `selfupdate.rs`. Dopo il controllo, il proprietario preme «Scarica aggiornamento»: il Core scarica in background l'installatore della piattaforma (`JARVIS_<v>_x64-setup.exe` su Windows, `JARVIS_<v>_aarch64.dmg` su macOS Apple Silicon) e il file `SHA256SUMS` della stessa release, verifica il SHA-256 e solo allora lo rende disponibile. «Installa e riavvia» riverifica il file e avvia uno script staccato che attende la chiusura dell'app, installa e riapre: Windows `setup /S` (installazione per utente, stessa cartella), macOS copia di `JARVIS.app` dal `.dmg` con sostituzione atomica (`.new` → `.old`). I dati non si toccano; il backup prima dell'aggiornamento resta quello di ADR-0028.
- **Sicurezza:** solo dal computer del Core (mai da dispositivi, 403 `owner_only`); su macOS richiede Touch ID/password come le altre operazioni sensibili; solo HTTPS verso `api.github.com`, `github.com` e `*.githubusercontent.com`, reindirizzamenti seguiti a mano e riconvalidati, token GitHub inviato solo ad `api.github.com`; massimo 400 MB; file `.part` rinominato dopo la verifica; percorsi con caratteri di shell rifiutati; audit `updates.download` e `updates.install`.
- **Limiti:** la verifica protegge da file corrotti o scambiati lungo la strada, non da un repository compromesso: restano le firme Sigstore di ADR-0020 da controllare a mano. Linux e Windows ARM restano manuali. L'APK Android non si aggiorna da qui (richiede il consenso di sistema all'installazione): è un lavoro a parte.
- **Non verificato:** installazione reale su Windows e macOS (script solo testati come testo; il flusso di download è provato con un server locale); l'avvio dell'installer dipende dai permessi dell'utente.
- **Evidenza:** test del piano degli asset, di `SHA256SUMS`, degli host ammessi, del percorso dell'app, degli script, di un file manomesso e di un download completo da server locale con checksum corretto e sbagliato; build egui.
- **Documenti interessati:** ROADMAP.md, DEVOPS_QA_OBSERVABILITY.md.

### ADR-0110 — Risposte più chiare dei modelli

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout.
- **Decisione:** il prompt di sistema aggiunge la data e l'ora locali (i modelli non le conoscono), chiede testo semplice adatto a una finestra di chat (niente tabelle Markdown né intestazioni decorative, codice in blocchi), una lettura dichiarata invece di domande a vuoto quando una richiesta vaga ha un'interpretazione chiaramente più probabile, e spiegazioni in italiano semplice per errori e limiti degli strumenti. I messaggi di errore di rete ora dicono la causa (ADR-0108).
- **Evidenza:** test del prompt di sistema.
- **Documenti interessati:** ROADMAP.md.

### ADR-0111 — Plancia rifatta da capo (desktop e macOS)

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («dashboard e grafica ancora brutta e buggata, rifalla da capo»).
- **Problemi della versione precedente:** un riquadro con altezza fissa (292 px) in cui testo, avatar e pulsanti erano posizionati a mano e si sovrapponevano a certe larghezze; tre layout diversi per la stessa intestazione; metriche non cliccabili; nessuna indicazione di cosa fare.
- **Decisione:** nuova Plancia con un solo flusso verticale e nessuna altezza fissa del contenuto: (1) intestazione con titolo di stato («Tutto in ordine» / «N cose richiedono la tua attenzione»), pillole Core e AI, avatar in uno spazio riservato e scorciatoie; (2) **«Da guardare»**: elenco ordinato di ciò che richiede il proprietario (conferme in attesa, registro di audit non integro, Core o AI non disponibili, Core collegato offline, nuova versione, notifiche, configurazione iniziale), ciascuna con il pulsante che porta alla pagina giusta, oppure «nessuna azione richiesta»; (3) sei metriche in una griglia che si adatta alla larghezza (1–3 colonne), cliccabili; (4) Core collegati, attività e notifiche recenti come prima. L'opzione «Riduci animazioni» scende in fondo.
- **Dati:** la Plancia legge anche l'esito dell'ultimo controllo aggiornamenti (nessuna nuova richiesta a GitHub). Su macOS si aggiungono le metriche Intelligenza artificiale e Notifiche.
- **Non verificato:** la resa a schermo (nessuna schermata vista in questo ambiente: clippy e test egui passano, Swift si compila solo in CI); Android non è stato toccato in questa fase.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0112 — Aggiornamento dell'app Android dall'interno

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («gli aggiornamenti direttamente all'interno»), seguito di ADR-0109.
- **Decisione:** la scheda «Aggiornamenti» della Plancia Android controlla le release su GitHub, scarica `JARVIS_<v>_android-release.apk` e `SHA256SUMS` solo via HTTPS da `github.com` e `*.githubusercontent.com` (reindirizzamenti seguiti a mano e riconvalidati, massimo 200 MB), verifica il SHA-256 e consegna l'APK al sistema con `FileProvider` limitato alla cartella `updates/` della cache. Android chiede il consenso all'installazione e accetta solo un APK firmato con la stessa chiave dell'app: è la garanzia di autenticità. Il permesso `REQUEST_INSTALL_PACKAGES` serve solo ad avviare la richiesta; se «Installa app sconosciute» non è consentito, l'app apre la schermata di sistema dove abilitarlo.
- **Logica:** `AppUpdate` nel modulo `core-client` (JVM, testata): confronto delle versioni, scelta della release stabile più recente con APK e checksum, host ammessi, parsing di `SHA256SUMS`, hash e download verificato.
- **Plancia Android:** il titolo di stato dice «Tutto in ordine» oppure quante notifiche aspettano; la scheda aggiornamenti compare nella Plancia.
- **Limiti:** serve una repository raggiungibile senza credenziali (con repository privata il controllo fallisce con un messaggio e l'aggiornamento resta manuale); nessun controllo automatico in background, solo su richiesta.
- **Non verificato:** compilazione e resa dell'interfaccia Compose (solo CI) e l'installazione reale su un telefono.
- **Evidenza:** `AppUpdateTest` (versioni, scelta della release, host, checksum e hash).
- **Documenti interessati:** ROADMAP.md, INTERFACE_AND_DEVICES.md.

### ADR-0113 — Diagnosi di Docker con la correzione da fare

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora ulteriormente Docker»).
- **Problema:** quando Docker non andava, sandbox, `code.run_python` e n8n rispondevano allo stesso modo («Docker non disponibile»), senza dire se mancasse l'installazione, se il motore fosse spento o se mancassero i permessi.
- **Decisione:** `container_runtime::diagnose()` interroga il motore con `docker version` (massimo 6 s) e classifica l'esito: non installato, motore spento, permesso negato, sconosciuto, pronto; `classify` è una funzione pura provata nei test (messaggi di Linux, macOS e Windows). Ogni stato ha un messaggio con la correzione («avvia Docker Desktop», «aggiungi l'utente al gruppo docker»). `sandbox.status`, l'errore di `sandbox.run` e lo stato di n8n riportano stato, versione del motore e messaggio.
- **Evidenza:** test della classificazione; nessun Docker in questo ambiente, quindi la diagnosi su un motore reale non è stata provata.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0114 — Preparazione dell'immagine del sandbox dall'app

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout.
- **Problema:** l'immagine `jarvis-sandbox` si costruiva con `infra/sandbox/build.sh`, ma un'app installata non ha il repository.
- **Decisione:** il Dockerfile è incorporato nel Core. Il nuovo strumento `sandbox.prepare` («prepara la sandbox», con conferma perché scarica dalla rete) costruisce l'immagine in background con `docker build -` (nessun contesto, solo il Dockerfile), massimo 15 minuti; lo stato (`idle`, `building`, `ready`, `failed`) e la causa di un errore (rete assente, disco pieno, accesso negato) compaiono in `sandbox.status`. Resta l'unico momento in cui il sandbox usa la rete: i container dei comandi restano senza rete. Escluso da missioni e automazioni.
- **Evidenza:** test dell'interpretazione dei log di costruzione, del Dockerfile incorporato, dei descrittori e della regola del gateway.
- **Non verificato:** una costruzione reale.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md.

### ADR-0115 — Etichette e pulizia dei container del sandbox

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout.
- **Decisione:** ogni container creato dal sandbox porta l'etichetta `jarvis.sandbox=1` ed è avviato con `--ipc none` (nessuna memoria condivisa con altri container). All'avvio il Core elimina in un thread a parte i container con quell'etichetta rimasti dopo un arresto brusco; gli identificativi di `docker ps -q` sono validati (solo esadecimali). I container di n8n non portano l'etichetta e non vengono toccati.
- **Evidenza:** test degli argomenti di `docker run` e dell'analisi degli identificativi.
- **Documenti interessati:** SECURITY_AND_PRIVACY.md, ROADMAP.md.

### ADR-0116 — Permessi del proprietario: profili e regole per strumento

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora i permessi di JARVIS»).
- **Problema:** la policy dipendeva solo dal descrittore dello strumento (rischio e conferma): il proprietario non poteva rendere JARVIS più prudente, né vietare uno strumento, senza cambiare il catalogo.
- **Decisione:** `permissions.rs` e `<dati>/permissions.json`. Tre profili: **Standard** (comportamento del catalogo), **Prudente** (ogni strumento che modifica o esegue chiede la conferma), **Solo lettura** (tutto ciò che modifica o esegue è negato). In più regole per strumento o per prefisso (`connector.*`): `ask` (conferma a ogni uso) o `deny`; la più severa vince. La policy applica le regole prima delle conferme del catalogo, con i motivi `policy.owner_rule_deny` e `policy.owner_rule_ask`, spiegati in italiano.
- **Invariante di sicurezza:** si può solo **restringere**; nessun profilo o regola toglie una conferma richiesta dal catalogo o rende consentito ciò che la policy nega. Un file illeggibile fa scegliere il profilo Prudente (fallire chiuso).
- **Controllo:** `GET/PATCH /api/permissions` solo dal computer del Core (403 per i dispositivi); le regole devono riferirsi a strumenti del catalogo; ogni modifica è nell'audit (`permissions.update`). Non c'è nessuno strumento di chat per cambiarle, così un testo ostile non può allargare né ridurre i permessi. Pagina «Permessi» nell'app desktop con profilo e regole per ogni strumento.
- **Limiti:** la schermata macOS non c'è ancora (le regole valgono comunque, si gestiscono dall'API o dal desktop); le automazioni già salvate che usano uno strumento con regola `ask` si fermano alla richiesta di conferma diretta.
- **Evidenza:** test unitari (profili, prefissi, la più severa vince, input non valido, file corrotto) e `tests/permissions.rs` con il Core (conferma richiesta, negazione, sola lettura che non blocca le letture, elenco con restrizioni).
- **Documenti interessati:** SECURITY_AND_PRIVACY.md, AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0117 — Strumenti di competenza per aziende, informatica e sicurezza

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («specialità nelle aziende, in informatica e in cyber security»).
- **Decisione:** cinque strumenti di calcolo o sola lettura, senza rete e senza scritture (`expert.rs`): `security.cvss` (punteggio base CVSS v3.0/3.1 e gravità da un vettore), `security.hash` (SHA-256 di un file della workspace con confronto con un valore atteso, per verificare scaricamenti e immagini), `it.subnet` (rete, maschera, broadcast, host utilizzabili di una sottorete IPv4), `biz.vat` (IVA: aggiunge o scorpora, aliquota predefinita 22%) e `biz.loan` (rata costante, totale e interessi). Il calcolo non lo fa il modello: lo fa lo strumento, e il risultato è verificabile.
- **Regole offline:** «cvss CVSS:3.1/…», «sottorete 192.168.1.0/24», «sha256 file [atteso <hash>]», «iva al 22% su 1.000,50», «rata mutuo 100.000 € al 3,5% in 20 anni»; i numeri si leggono all'italiana.
- **Evidenza:** punteggi CVSS pubblicati (9,8, 10,0, 6,1, 7,8, 0), sottoreti, hash noto (`abc`), IVA e rata di un mutuo di riferimento (579,96 €), percorsi fuori dalla workspace rifiutati.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0118 — Specialisti e competenze per aziende, IT e sicurezza difensiva

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout.
- **Decisione:** nuovo specialista **business** (`delegation.schema.json` ammette il nome) con IVA, finanziamenti, note, cose da fare, lettura di file e ricerca Web, e nessuna scrittura né terminale. Lo specialista della sicurezza (`monitoring`) riconosce anche CVSS, vulnerabilità, ransomware, phishing, incidenti e violazioni dei dati e ottiene `security.cvss` e `security.hash`; lo specialista di sistema riconosce sottoreti e runbook e ottiene `it.subnet`. Quattro nuove competenze predefinite: **Analisi e procedure aziendali**, **Operazioni IT e runbook**, **Risposta agli incidenti di sicurezza** (contenere, preservare le prove, valutare, ripristinare, comunicare; ricorda la notifica entro 72 ore del GDPR), **Conformità e protezione dei dati** (GDPR, NIS2, ISO/IEC 27001, senza valore legale). Restano difensive: nessun exploit, scansione di sistemi terzi o azione offensiva.
- **Limiti:** le competenze sono istruzioni per il modello, non consulenza professionale: ogni testo lo dice quando serve un legale, un DPO o un commercialista.
- **Evidenza:** test di instradamento verso gli specialisti e dei loro strumenti concessi.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0119 — Collegamento con chiave API affidabile

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («il collegamento tramite API Key funzionante al 100%»).
- **Problemi trovati:** (1) la chiave incollata veniva rifiutata, o salvata sbagliata, se aveva «Bearer », «Authorization:», `export NOME=`, virgolette, a capo o caratteri invisibili (BOM, spazi a larghezza zero) copiati dai siti; (2) «Testa chiave» faceva una chat di prova e un errore di modello poteva sembrare un errore di chiave; (3) un deposito dei segreti che non salva dava errori misteriosi solo alla prima chat; (4) nessun avviso se la chiave era di un altro servizio.
- **Decisione:** `normalize_key` ripulisce la chiave incollata (prefissi, virgolette, caratteri invisibili) e rifiuta con il motivo esatto spazi interni, caratteri non ASCII, chiavi vuote o troppo corte. Dopo il salvataggio la chiave viene riletta e confrontata; se non coincide, errore esplicito. `key_format_hint` avvisa (senza bloccare) se una chiave NVIDIA non inizia con `nvapi-` o se una chiave Ollama sembra di un altro servizio. «Testa chiave» ora verifica prima le **credenziali** con l'elenco dei modelli del provider (`GET /api/tags` per Ollama Cloud, `GET /v1/models` per NVIDIA NIM: nessuna chat consumata, 401/403/402/429/rete distinti con retry sugli errori transitori), poi fa la richiesta di prova, e risponde anche con il numero di modelli disponibili e se il modello scelto è fra quelli elencati. Se l'elenco non è offerto dal servizio si prosegue con la sola prova di chat.
- **Limiti:** la verifica dell'elenco usa i due indirizzi pubblici dei provider; una rete che blocca `ollama.com` o `integrate.api.nvidia.com` ora lo dice con la causa (ADR-0108). Non ho potuto provare chiavi reali (nessuna disponibile in questa sessione): gli indirizzi e il formato delle risposte sono quelli documentati dai provider, provati con risposte simulate.
- **Evidenza:** test di `normalize_key` (otto formati incollati, quattro rifiuti), di `list_models` con server locale (200, 401, 403) e dell'analisi delle risposte dei due provider.
- **Documenti interessati:** FIREBASE_SETUP.md non coinvolto; AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0120 — Personalità «come nel film» e chat più chiare

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora la personalità e fai sì che sia come quella del film»).
- **Decisione:** `persona.rs` e `<dati>/persona.json`. Due stili: **film** (predefinito: maggiordomo digitale impeccabile, pacato e preciso, leggermente britannico, ironia asciutta e rara, anticipa le esigenze, rapporti di stato brevi, riserva garbata davanti a un'idea rischiosa e poi esegue ciò che il proprietario decide entro le regole, formule brevi come «Certamente.» e «Fatto.», mai servile, nessuna battuta o dialogo copiato) e **sobrio** (neutro ed essenziale). La **forma di rivolgersi** è vuota per impostazione predefinita: JARVIS non sa come vuoi essere chiamato e non assume «signore» o «signora»; si imposta (per esempio «Signore», «Signora» o il nome) nella pagina Modelli AI dell'app desktop o con `PATCH /api/persona`, solo dal computer del Core. Lo stile entra nel prompt di sistema accanto a data e ora; non tocca policy, permessi o conferme.
- **Chat:** il prompt (ADR-0110) resta la base: testo semplice, lettura dichiarata se la richiesta è vaga, errori spiegati. Gli errori di connessione dicono la causa (ADR-0108).
- **Limiti:** lo stile vale per le risposte dei modelli cloud; le risposte a regole offline restano neutre.
- **Evidenza:** test della personalità predefinita, della validazione e del salvataggio, del prompt di sistema.
- **Documenti interessati:** ROADMAP.md.

### ADR-0121 — JARVIS apre le applicazioni e le usa al posto del proprietario

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («la possibilità di aprire applicazioni e usarle al posto tuo»). Estende `pc.open` (ADR-0053).
- **Decisione:** `appcontrol.rs` e cinque strumenti: `app.open` (apre un'app installata per nome), `app.focus` (la porta in primo piano), `app.windows` (elenca app e finestre aperte, sola lettura), `app.type` (scrive un testo con la tastiera nell'app in primo piano, fino a 500 caratteri) e `app.key` (preme un tasto o una scorciatoia: lettera, cifra, invio, tab, esc, spazio, canc, frecce, con command/control/option/shift). macOS: `open -a` e `osascript` con System Events; Windows: PowerShell (`Start-Process`, `AppActivate`, `SendKeys`).
- **Sicurezza:** ogni azione che agisce sul computer richiede la conferma del proprietario, e solo il proprietario dal computer del Core può darla; la schermata di conferma mostra il testo esatto che sarà scritto. Il nome dell'app ammette solo lettere, cifre, spazi e `.+_-`. **I valori non entrano mai nel testo di uno script:** passano come argomenti di `osascript` (`argv`) o come variabili d'ambiente di PowerShell; i soli frammenti variabili vengono da tabelle fisse. Nessuna credenziale o chiave può essere digitata (`app.sensitive`). Il processo non eredita token né segreti. Dispositivi associati, automazioni, nodi, missioni e agenti temporanei non possono usarli; i modelli cloud non li vedono se la richiesta non è una richiesta diretta del proprietario; i permessi del proprietario (ADR-0116) possono vietarli con `app.*`. Il tempo massimo è 10 secondi.
- **Regole offline:** «apri l'app Spotify», «apri Spotify» (nome con iniziale maiuscola), «avvia il programma …», «porta in primo piano …», «scrivi nell'app: …», «premi cmd+shift+s», «quali finestre sono aperte».
- **Limiti:** JARVIS non vede lo schermo (nessun modello di visione): non può cliccare in un punto né leggere il contenuto di una finestra, solo scrivere, premere tasti e sapere quali finestre esistono. macOS richiede il permesso Accessibilità (l'errore indica dove abilitarlo); Windows non può scrivere nelle finestre avviate come amministratore. Non disponibile su Linux. Il contratto di delega ammette ora fino a 96 strumenti (il Core ne propone al massimo 64 per richiesta).
- **Non verificato:** l'esecuzione reale su macOS e Windows (non disponibili qui): sono provati la costruzione dei comandi per entrambi i sistemi, l'assenza di iniezione, la conferma e i divieti.
- **Evidenza:** test unitari di nomi, argomenti, piani per macOS e Windows con testo ostile, mappe dei tasti, errori e finestre; `tests/appcontrol.rs` con il Core (conferma richiesta per ogni azione, testo mostrato, credenziali rifiutate, divieto dai permessi); test del gateway.
- **Documenti interessati:** SECURITY_AND_PRIVACY.md, AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0122 — Lettura privata dello schermo

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («permetti di vedere lo schermo, in maniera privata, senza condivisione di dati sensibili o personali»).
- **Decisione:** `screen.read` cattura lo schermo con gli strumenti del sistema (`screencapture` su macOS, PowerShell su Windows), lo legge con **OCR locale** (Tesseract, `ita+eng`) e **cancella subito l'immagine**, anche in caso di errore: l'immagine non viene salvata, né inviata, né messa in memoria o nell'audit; restano solo il testo e il conteggio dei dati mascherati. `screen.status` (sola lettura) dice se l'OCR è installato e cosa manca.
- **Privacy:** (1) ogni lettura richiede la conferma del proprietario, mostrata prima («catturare lo schermo, leggerlo in locale ed eliminare subito l'immagine»); (2) il testo passa da `privacy::redact`, che maschera e-mail, chiavi e segreti, numeri di carta (controllo di Luhn), IBAN (mod 97), codice fiscale, telefoni, lunghe sequenze di cifre, indirizzi IP e i valori dopo «password», «PIN», «CVV» o «OTP»; (3) il testo dello schermo non viene mai passato ai modelli cloud: i modelli cloud non hanno lo strumento e una richiesta con credenziali è già bloccata; (4) cartella temporanea 0700, file con più di 10 minuti e residui dopo un arresto brusco eliminati all'avvio; (5) vietato a dispositivi associati, automazioni, missioni e agenti temporanei, e vietabile dai permessi (`screen.*`).
- **Limiti:** nomi, indirizzi postali e testo libero non sono riconoscibili in modo affidabile: il mascheramento riduce l'esposizione, non la elimina, per questo la conferma resta a ogni lettura. L'OCR legge testo, non descrive immagini o interfacce; non ci sono clic. Serve Tesseract (macOS: `brew install tesseract tesseract-lang`; Windows: `winget install UB-Mannheim.TesseractOCR`) e su macOS il permesso «Registrazione schermo». Non disponibile su Linux.
- **Non verificato:** cattura e OCR reali su macOS e Windows (non disponibili qui): sono provati la costruzione dei comandi, il mascheramento, la pulizia, la conferma e i divieti.
- **Evidenza:** `privacy` (dati sensibili mascherati, testo ordinario intatto, validatori contro i falsi positivi), `screen` (comandi per i due sistemi, pulizia, cartella privata), `tests/appcontrol.rs` (conferma richiesta con il testo sulla riservatezza, stato leggibile senza conferma).
- **Documenti interessati:** SECURITY_AND_PRIVACY.md, AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0123 — Voce solo maschile e comandi vocali migliori

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora i comandi vocali e la sua voce, solamente MASCHILE»). Sostituisce la voce predefinita di ADR-0010 (`it_IT-paola-medium`, femminile).
- **Voce maschile, in ogni percorso:** (1) Piper: elenco ridotto a `it_IT-riccardo-x_low` (predefinita) e `en_US-ryan-medium`; il Python rifiuta una voce con un nome femminile noto (`voice.not_male`, con il rimedio) e accetta una voce ignota solo se dichiarata maschile con `JARVIS_TTS_VOICE_GENDER=male`; XTTS richiede la stessa dichiarazione; chi aveva salvato una voce non più ammessa passa a Riccardo (va rieseguito `voice-setup` per scaricarla). (2) **Voce di sistema** (`speech.rs`, nuova, senza modelli): macOS `say` con «Luca» (preferendo Premium ed Enhanced), Windows `System.Speech` con `VoiceGender.Male` e verifica del genere; **mai un ripiego su una voce femminile**: senza una voce maschile installata JARVIS tace e dice come installarla.
- **Lettura delle risposte:** impostazione «Leggi le risposte ad alta voce» (spenta di default) nella pagina Modelli AI di desktop e macOS, `GET/PATCH /api/voice/system`, `POST /api/voice/say` e `/api/voice/stop`, solo dal computer del Core. Il testo è preparato per la voce: niente markdown, codice, indirizzi o emoji; «%», «€», «°C» e unità lette a parole; il testo passa come argomento o variabile d'ambiente, mai dentro uno script; una nuova lettura interrompe la precedente.
- **Voce di Piper:** livellamento del volume, dissolvenze di 25 ms e niente silenzio in coda, e stessa preparazione del testo.
- **Comandi vocali:** la parola di attivazione tollera le trascrizioni frequenti («Giarvis», «Garvis», «Jervis»…); i numeri a parole davanti a un'unità diventano cifre («tra venti minuti» → «tra 20 minuti», fino a 999); «stop», «basta», «zitto», «ripeti», «annulla» sono riconosciuti come comandi di controllo solo se sono l'intera frase (`intent`); più parole chiave per il riconoscimento (sandbox, missione, conferma, promemoria…). I contratti di trascrizione ammettono `normalized` e `intent`.
- **Limiti:** le app non registrano ancora dal microfono (il riconoscimento è disponibile come servizio del Core); «Luca» e «Cosimo» vanno installati dal sistema se non presenti; i comandi vocali non possono confermare azioni a rischio.
- **Non verificato:** l'ascolto di una voce reale (nessun audio in questo ambiente) e `say`/`System.Speech` sui sistemi reali: sono provati la scelta della voce, il testo, i comandi costruiti e i divieti.
- **Evidenza:** test Python (voce maschile e rifiuto di quelle femminili, numeri, intenti, attivazione, testo da leggere, livellamento), test Rust (`speech`, `setup`) e dei contratti.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0124 — Database e memoria più robusti e veloci

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora il database e la memoria»).
- **Database:** pragma di prestazioni (cache da 16 MiB, `mmap` da 128 MiB, limite del file di journal a 64 MiB) oltre a WAL, `busy_timeout` e `synchronous=NORMAL` già presenti. La manutenzione periodica ora compatta il file (`VACUUM`) solo quando conviene, cioè oltre il 25% di pagine libere e almeno 8 MiB recuperabili. Conservazione: tracce di esecuzione oltre 60 giorni e notifiche **lette** oltre 120 giorni vengono eliminate (le non lette restano). Lo stato del database riporta spazio libero e coerenza degli indici di memoria.
- **Memoria, coerenza:** gli indici derivati (ricerca testuale e vettori) sono confrontati con la tabella dei ricordi all'avvio; se dopo un arresto brusco un elemento manca in un indice o ce n'è uno orfano, gli indici vengono ricostruiti dalla sola tabella `memory`. Prima si controllava solo l'assenza dei vettori.
- **Memoria, ricerca:** il costo non cresce più con il testo di ogni ricordo: il coseno si calcola sui soli vettori e testo e frasi si valutano sui candidati (i 300 più vicini più i risultati della ricerca testuale). I ricordi più recenti passano avanti a parità di pertinenza (mezza vita di un anno, al massimo 0,04): vale solo per l'ordine, mai per la soglia di pertinenza. Con 3000 ricordi la ricerca resta sotto i due secondi nel test.
- **Nuove letture:** `GET /api/memory/stats` (ricordi per tipo, in scadenza entro 7 giorni, relazioni, più vecchio e più recente, indici coerenti) e `GET /api/memory/duplicates?threshold=` (coppie di ricordi quasi uguali, solo da leggere: decide il proprietario cosa dimenticare). Nessuna eliminazione automatica: due ricordi simili possono avere sfumature diverse.
- **Limiti:** la ricerca vettoriale resta lineare sul numero di ricordi (adeguata fino a decine di migliaia); gli embedding sono quelli locali già in uso.
- **Evidenza:** test su indici interrotti e orfani, riepilogo e doppioni senza eliminazioni, 3000 ricordi con ricerca corretta e rapida, ordinamento per recenza senza superare la pertinenza, soglie di compattazione e potatura delle notifiche.
- **Documenti interessati:** ARCHITECTURE.md non toccato; ROADMAP.md.

### ADR-0125 — Più animazioni e grafica nell'app macOS

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora ancora la grafica macOS con SwiftUI e aggiungi più animazioni»).
- **Decisione:** nuovo `Motion.swift` con componenti riusabili: `PressableStyle` (il pulsante si abbassa), `PulsingDot` (onda di stato), `TypingIndicator` (tre punti che rimbalzano), `AuroraGlow` (bagliore lento dietro l'intestazione), `StaggeredAppear` e `HoverLift`. Applicati a: barra laterale (evidenziazione che scorre fra le voci con `matchedGeometryEffect`, evidenziazione al passaggio del puntatore, pressione), passaggio fra sezioni (dissolvenza con piccolo scorrimento), Plancia (intestazione con bagliore e punto di stato pulsante, schede e riquadri che entrano in sequenza, riquadri che si sollevano al passaggio del puntatore, valori che cambiano con `numericText`), campanella delle notifiche che rimbalza quando ne arrivano, chat (messaggi che salgono dal basso, indicatore di scrittura al posto della barra di avanzamento) e nuova scheda «Stato della memoria» con contatori animati.
- **Accessibilità:** con «Riduci movimento» attivo in macOS spariscono scorrimenti, rimbalzi, ingrandimenti e animazioni continue (onda, bagliore, punti): restano dissolvenze brevi. Gli elementi puramente decorativi sono nascosti a VoiceOver.
- **Non verificato:** la resa a schermo e la fluidità (nessuna schermata vista; SwiftUI si compila solo nella CI). Le animazioni continue usano `TimelineView` a 24–30 fotogrammi al secondo e si fermano con «Riduci movimento».
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0126 — Plancia macOS ispirata al mock-up e carichi del sistema

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («prendi spunto dalla foto e migliora la qualità dell'app macOS»).
- **Decisione:** la Plancia riprende la struttura del mock-up: saluto in base all'ora («Buongiorno / Sono JARVIS»), scheda **AI** (modello attivo e stato dell'Orchestratore), avatar con anelli in orbita e onda sonora (`OrbitRings`, `WaveformView`), scheda **Sistema** (CPU, RAM, archiviazione), scheda **Specialisti** (scorciatoie verso una chat già avviata), barra dei comandi «Parla con JARVIS…», sei categorie rapide, «Attività recenti» e «Missioni in corso». La barra laterale mostra il marchio, le voci evidenziate con sfumatura e, in fondo, la scheda «JARVIS Online / Modalità: Operativa». In testata compaiono lettura ad alta voce e ricerca nei ricordi.
- **Nuova lettura:** `GET /api/system/stats` → `{cpu_percent, memory_percent, disk_percent, cores, os}`. Legge solo `/proc` (Linux), `vm_stat`/`sysctl` (macOS) e lo spazio del volume dei dati; nessun nome di processo, percorso o dato personale. Cache di 3 s, eseguita in `spawn_blocking`; richiede il token come ogni altra API.
- **Non incluso:** le icone degli agenti del mock-up sono scorciatoie, non agenti separati: il Core resta l'unico orchestratore. Il modello mostrato è quello configurato, non una misura di latenza.
- **Non verificato:** resa a schermo (SwiftUI si compila solo nella CI macOS); i parser dei carichi sono testati con fixture sintetiche, la lettura reale solo su Linux.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0127 — Chat, riconoscimento vocale (STT), sintesi (TTS) e parola di attivazione migliori

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora le chat, la wake word ed entrambi i modelli TTS e STT»).
- **Chat (macOS):** le risposte di JARVIS mostrano markdown (grassetto, corsivo, codice in linea, elenchi) e blocchi di codice con intestazione del linguaggio e pulsante «Copia» (`MessageContent`). I collegamenti si aprono solo se sono `http`/`https`. Il campo di scrittura è una capsula a più righe con pulsante rotondo di invio.
- **STT:** pre-elaborazione dell'audio (rimozione del livello continuo, guadagno limitato, taglio dei silenzi), soglia sul rapporto di parlato prima della trascrizione, `beam_size=5`, soglie `no_speech`/`log_prob`/compressione, `initial_prompt` con il lessico di JARVIS, filtro delle allucinazioni tipiche («sottotitoli…», «grazie per la visione») e dei segmenti ripetuti.
- **Wake word:** la decisione combina punteggio del modello e trascrizione (`confirmed_by`: `score`, `transcript`, `score+transcript`) e scarta l'audio con poco parlato (`speech_ratio`), per ridurre falsi positivi con rumore o musica. Contratto `jarvis.voice-transcription/1` esteso in modo compatibile.
- **TTS:** il testo viene diviso in frasi e sintetizzato frase per frase con pause naturali; Piper usa `SynthesisConfig` esplicita. Restano solo voci **maschili** (decisione ADR-0123).
- **Privacy:** audio e trascrizioni restano locali; nessun nuovo invio a servizi esterni.
- **Non verificato:** accuratezza reale su microfono e voce vera (test con segnali sintetici); le soglie vanno tarate sull'hardware del proprietario.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0128 — Avatar olografico animato su macOS

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («concentrati sull'avatar, fatto bene e molto animato»).
- **Decisione:** `Avatar.swift` è riscritto come un'unica scena `Canvas` a 30 fps, interamente vettoriale (nessuna immagine). Elementi: alone che respira; onde che si propagano dal centro; anello tratteggiato, archi controrotanti e 72 tacche con onda di luce; 42 scintille in orbita con scintillio individuale; testa con lieve oscillazione, inclinazione e respiro; fascio di scansione ritagliato sul volto; occhi con bagliore, palpebre e sguardo a scatti (cercano avanti e indietro mentre elabora); bocca a sette barre; nucleo sul petto che pulsa e ruota; glitch rapido ogni ~9 s.
- **Stati:** a riposo, **elabora** (`thinking`: anelli più veloci, arco di scansione, occhi più stretti) e **parla** (`speaking`: sillabe sulla bocca, onde più fitte). I passaggi sono continui (`Animatable`), non a scatto. «Parla» dura quanto la lettura stimata (≈14 caratteri/s), perché il Core non segnala la fine della riproduzione.
- **Interazione:** al passaggio del puntatore l'avatar si ingrandisce con una molla e guarda in avanti; al clic emette un'onda.
- **Accessibilità:** con «Riduci movimento» la scena resta ferma e senza glitch; VoiceOver legge lo stato.
- **Non verificato:** resa e fluidità (nessuna schermata; SwiftUI solo in CI). Costo CPU da misurare su Mac reale: se eccessivo, abbassare i fotogrammi o le scintille.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0129 — Avatar su Windows e Android, barre laterali migliorate

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («adatta l'avatar anche a Windows e Android e migliora la sidebar dell'app»).
- **Avatar:** la scena di ADR-0128 (alone, onde, anelli con tacche, scintille, scansione, sguardo, palpebre, bocca a barre, nucleo, glitch) è portata sul client desktop egui (`apps/desktop/src-tauri/src/avatar.rs`, usato dalla Plancia a 132 px) e su Android (`JarvisAvatar.kt`, Plancia, 248 dp). Stati continui riposo/elaborazione/parlato; clic o tocco emettono un'onda. Su Windows «parla» per la durata stimata della lettura (≈14 caratteri/s, perché il Core non segnala la fine della riproduzione); su Android non c'è lettura vocale, quindi gli stati sono riposo ed elaborazione. Con «Riduci animazioni» (egui) o animazioni di sistema a zero (Android) la scena resta ferma. Android disegna l'avatar su un disco scuro per restare leggibile anche con il tema chiaro e limita il disegno a 30 fotogrammi al secondo.
- **Sidebar Windows (egui):** pagine raggruppate come su macOS (Assistente, Organizzazione, Connessioni, Protezione, Sistema); marchio animato in cima; voci disegnate con icone geometriche (nessun glifo di font che possa mancare), evidenziazione al passaggio e selezione con transizione e barra laterale accesa; scheda di stato in fondo con punto che pulsa, modalità (Operativa/Al lavoro) e onda sonora. L'ordine di `NAV` segue i gruppi.
- **Android:** la barra inferiore usa `JarvisTabBar`: icone disegnate, pillola che si allarga con una molla, icona che salta e pallino con le notifiche da leggere (aggiornato ogni 20 s con l'app in primo piano).
- **macOS:** la barra laterale mostra un contatore accanto a «Conferme» e «Notifiche» quando ci sono elementi in attesa.
- **Non incluso:** contatori nella barra di Windows (la pagina corrente non conosce lo stato aggregato senza una chiamata in più).
- **Non verificato:** resa e fluidità su schermo; il Kotlin si compila solo in CI (nessun Android SDK qui) e non ho eseguito il client egui su Windows. Verificati: fmt, clippy `-D warnings` e 23 test del client egui su build Linux con stub (funzioni pure dell'avatar comprese).
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0130 — Risposte più rapide e meno generiche

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («ci mette troppo a rispondere e dà risposte troppo vaghe»).
- **Cause trovate:** (1) la chat cloud partiva **senza i turni precedenti** salvo spuntare ogni volta la casella (che si azzerava a ogni invio): il modello non vedeva la conversazione e rispondeva a vuoto; (2) la scelta del modello mandava richieste ordinarie ai modelli di ragionamento, molto più lenti: la complessità includeva tutta la cronologia, «prima» e «poi» (parole comuni) contavano come sequenza, bastavano 5 righe o la parola «design»; (3) una nuova connessione TLS a ogni messaggio; (4) un modello veloce senza risposta teneva l'utente fino a 90 s prima del ripiego; (5) le istruzioni chiedevano di essere «concisi» e di chiedere chiarimenti quando mancava un elemento, il che produce panoramiche e domande.
- **Decisione (contesto):** i turni recenti vanno al provider scelto **per impostazione predefinita** (scelta esplicita del proprietario, con interruttore «Ricorda la conversazione recente» su macOS e Windows, memorizzato e non più azzerato). Resta valido il filtro dei segreti; per i dispositivi collegati il contesto resta spento. Un `use_context` esplicito nella richiesta prevale.
- **Decisione (velocità):** complessità misurata sulla sola richiesta (≥420 token, ≥8 righe, termini di analisi; «design» tolto); due parole di sequenza vere (`poi`, `infine`, …) invece di «prima/poi»; la cronologia conta solo per il contesto lungo (≥3200 token). Agente HTTP condiviso per riusare le connessioni. Timeout del modello veloce a 40 s (poi subentra l'alternativa); 90 s per gli altri.
- **Decisione (qualità):** prompt di sistema con «sii specifico» (cifre, nomi, comandi, passaggi, esempi; niente consigli di rito; rispondere nel merito), ipotesi più probabile dichiarata invece di domande a vuoto, chiarimento richiesto solo per file, persone o azioni ambigue; guida del percorso veloce «diretta e completa» al posto di «concisa».
- **Non incluso:** risposta in streaming per la chat cloud (richiede flussi per entrambi i provider, anche con chiamate a strumenti): è il prossimo passo per ridurre l'attesa percepita. Memoria personale del proprietario nel contesto cloud: non inviata (decisione di privacy da prendere a parte).
- **Non verificato:** latenza e qualità reali (nessuna chiave API nel checkout): servono prove con i provider veri. Verificati con fixture: routing, prompt, contesto predefinito, test del Core.
- **Documenti interessati:** AI_SYSTEM.md, ROADMAP.md.

### ADR-0131 — Chat cloud in streaming

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («risolvi gli errori di CI e migliora ulteriormente questo software, ancora troppo basico»); seguito di ADR-0130.
- **Problema:** la chat cloud aspettava la risposta intera prima di mostrare qualcosa: con i modelli lenti l'attesa percepita era di decine di secondi.
- **Decisione:** nuovo `POST /api/cloud/chat/stream` (NDJSON come `/api/chat/stream`): righe `{"type":"delta","text":…}` mentre il modello genera, poi `{"type":"done","result":…}` (stesso oggetto di `/api/cloud/chat`) oppure `{"type":"error",…}`. Il Core legge NDJSON (Ollama) e SSE (NVIDIA NIM), inoltra i frammenti e ricostruisce lo stesso esito di una risposta completa, con gli stessi controlli (vuota, troncata, limite di byte). La logica è unica per i due endpoint (`run_cloud_chat`): stessi controlli su credenziali, dispositivi, comandi locali, saturazione e registrazione del turno.
- **Ripiego:** con strumenti proposti dal modello o con il modello segnato non disponibile si usa il percorso completo (il testo arriva in un blocco); se lo streaming fallisce **prima del primo frammento** si ripete con la richiesta completa e il modello alternativo; dopo il primo frammento l'errore è riportato com'è (niente doppie risposte).
- **Client:** macOS (`CoreSession.stream`) e Windows (`ApiClient::cloud_chat_stream`) mostrano il testo che cresce con cursore; l'avatar «parla» mentre il testo scorre. Android non usa la chat cloud del Core: invariato.
- **Sicurezza:** richiede il token del proprietario come ogni altra API; per i dispositivi collegati restano validi gli stessi limiti di `/api/cloud/chat` (contesto spento, azioni riservate al proprietario), anche se l'endpoint di streaming non è nell'elenco delle rotte ammesse ai dispositivi.
- **Non verificato:** comportamento con provider reali (formati verificati con server di prova per entrambi); resa SwiftUI e prova su Windows reale.
- **Documenti interessati:** AI_SYSTEM.md, INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0132 — JARVIS come assistente di direzione aziendale (CEO/CFO)

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («rendi ancora più professionale e specializzato JARVIS, un super CEO aziendale e gestionale»).
- **Problema:** lo specialista «business» aveva solo IVA e finanziamenti; la chat cloud, il percorso usato dall'app, non vedeva nessun calcolo aziendale (solo `math.calc`, `date.calc`, conversioni, e solo con la parola «calcola»): le cifre di un'analisi potevano uscire a memoria dal modello.
- **Decisione (strumenti, `executive.rs`):** sette calcoli deterministici, solo calcolo (senza rete, scritture o dati personali, rischio basso): `biz.margin` (margine, ricarico, prezzo da margine obiettivo), `biz.breakeven` (pareggio in unità e ricavi, profitto obiettivo, margine di sicurezza), `biz.npv` (VAN, TIR per bisezione, payback, indice di profittabilità), `biz.runway` (autonomia di cassa con crescita dei ricavi, soglie critico <6 mesi, attenzione <12), `biz.unit_economics` (LTV, LTV/CAC, payback del CAC), `biz.priorities` (RICE su fino a 30 iniziative), `biz.cagr` (crescita composta e proiezione). Input validati con intervalli, errori in italiano, risultati verificati su esempi svolti.
- **Decisione (specialista e competenze):** ruolo «Direzione aziendale (CEO/CFO)» con più parole chiave e i nuovi strumenti; quattro competenze incluse: **Direzione e decisioni** (formato: decisione, raccomandazione subito, evidenze con strumenti, opzioni, rischi, prossimi passi con responsabile e scadenza), **Controllo finanziario**, **Riunioni e azioni** (ordine del giorno, verbale, azioni), **Vendite e offerte**; la competenza «azienda» riceve gli stessi strumenti.
- **Decisione (chat cloud):** i nove strumenti aziendali (con IVA e finanziamenti) sono offerti al modello quando la richiesta contiene parole di calcolo aziendale (margine, pareggio, VAN, TIR, runway, LTV, CAC, CAGR, ROI, IVA, rata…); per le richieste aziendali il prompt aggiunge il formato da consulente di direzione e l'obbligo di usare gli strumenti per ogni cifra; le richieste aziendali di una certa sostanza (≥80 token) usano il modello di ragionamento. Comandi offline equivalenti (es. «punto di pareggio costi fissi … prezzo … costo variabile …», «runway cassa … costi … ricavi …», «van tasso 10% flussi -1000; 500; 500»).
- **Limiti:** nessun parere legale, fiscale o contabile vincolante (dichiarato nel prompt e nei ruoli); i calcoli usano i dati forniti dal proprietario, senza accedere a gestionali o conti; nessuna scrittura (le azioni nate da una riunione si salvano solo su richiesta, con le conferme già previste).
- **Non incluso:** collegamento a gestionali e contabilità (connettori dedicati) e report periodici automatici: possibili passi successivi.
- **Non verificato:** qualità delle analisi con provider reali; testati con fixture i calcoli, i comandi offline, i testi, la registrazione degli strumenti e il prompt.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0133 — Profilo del proprietario

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora il profilo utente»).
- **Decisione:** `profile.rs` salva in `<dati>/profile.json` ruolo, azienda, settore, competenze, interessi, obiettivi e tre preferenze (lingua, tono formale/cordiale/diretto, livello tecnico principiante/intermedio/esperto). `GET/PATCH /api/profile` solo per il proprietario, con audit (`profile.update`). Il riepilogo entra nel prompt di sistema (una riga, «usalo solo se pertinente e non citarlo») solo se «Usa il profilo nelle richieste cloud» è attivo; la percentuale di completamento e i suggerimenti dei campi mancanti guidano la compilazione.
- **Privacy:** niente nome completo né recapiti: email, indirizzi web, numeri di telefono (7+ cifre) e credenziali sono rifiutati sia in scrittura sia nel caricamento di un file modificato a mano (campo per campo). Tutto è facoltativo.
- **Client:** pagina «Profilo» su macOS, sezione nella pagina dei modelli su Windows. Android non lo modifica (i dispositivi non accedono al profilo).
- **Documenti interessati:** AI_SYSTEM.md, INTERFACE_AND_DEVICES.md.

### ADR-0134 — Ascolto sempre attivo «Hey Jarvis»

- **Data:** 2026-11-24 · **Stato:** implementata su macOS e nel Core; Windows in ADR-0140 · **Fonte:** richiesta del proprietario («rendi sempre attiva la wake-word… deve ascoltare solo Hey Jarvis, nient'altro»).
- **Decisione:** in attesa il microfono alimenta solo un buffer in memoria (40 s al massimo). I blocchi con voce (soglia di energia) vanno a `POST /api/voice/wake`, che risponde soltanto `{detected, engine, score}`: con il modello openWakeWord non viene trascritto nulla; senza modello la trascrizione serve solo a confrontarla con «hey Jarvis» e non esce dal worker. L'audio è scartato dopo il verdetto. Solo dopo un sì il comando viene registrato (fino a ~1 s di silenzio, massimo 12 s), trascritto ed eseguito; poi si torna all'ascolto della sola parola di attivazione. Nulla finisce su disco né in cloud.
- **Controlli:** impostazione `wake_listening` (attiva di default) in `/api/voice/system`, interruttore «Ascolto sempre attivo» nelle impostazioni vocali, indicatore del microfono nella barra laterale e avatar «in ascolto» con anelli sensibili al livello. L'ascolto si ferma con l'interfaccia bloccata, quando JARVIS parla o elabora (la sua voce non lo riattiva) e se il permesso del microfono è negato.
- **Permesso:** `NSMicrophoneUsageDescription` nell'`Info.plist`; macOS chiede il consenso al primo avvio.
- **Non incluso:** ascolto su Android (Windows: ADR-0140).
- **Non verificato:** prova con microfono e modello reali; test con audio sintetico per il verdetto (Python e Core) e per l'assenza di testo nell'esito.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0140 — Ascolto «Hey Jarvis» sul client Windows

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** scelta del proprietario («Ascolto su Windows») dopo ADR-0134.
- **Decisione:** stesso comportamento di macOS. `wake.rs` (portabile, testato su Linux) tiene il buffer circolare in memoria, invia a `POST /api/voice/wake` solo le finestre con voce nuova sopra soglia, registra il comando fino a ~1,1 s di silenzio, lo trascrive con `?mode=wake_word` e, se c'è solo «Hey Jarvis», attende un secondo comando; intenti «stop» e «annulla» rispettati. `mic.rs` (solo Windows) cattura il microfono predefinito con WASAPI tramite `cpal` 0.16, porta i campioni a 16 kHz mono e non scrive nulla su disco. Il comando arriva alla chat come messaggio dell'utente.
- **Controlli:** impostazione `wake_listening` (attiva di default) con interruttore «Ascolta sempre «Hey Jarvis»» nelle impostazioni vocali, stato e livello del microfono; l'ascolto è sospeso mentre la chat elabora o JARVIS parla; spento sui Core remoti (l'endpoint è riservato al proprietario).
- **Non incluso:** Android; prova con microfono e modello reali.
- **Evidenza:** fmt, Clippy Windows (`x86_64-pc-windows-gnu`, `--all-targets -D warnings`) e Clippy Linux con stub superati; 5 test di `wake` con trasporto simulato e audio sintetico (silenzio mai inviato, comando consegnato, altro parlato ignorato, «stop» zittisce la voce, ricampionamento 48 kHz stereo → 16 kHz mono). Il link dei test Windows non è stato eseguibile qui.
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, ROADMAP.md.

### ADR-0141 — Collegamento dei dispositivi con Tailscale in un gesto

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** scelta del proprietario («il collegamento tramite Tailscale va benissimo»), al posto della scoperta in LAN con QR (che avrebbe richiesto un listener TLS fuori da `127.0.0.1`).
- **Decisione:** il Core resta su `127.0.0.1`; Tailscale Serve lo pubblica con HTTPS privato nella rete del proprietario. `tailscale.rs` interroga il client installato (`status --json`, `serve status`) con argomenti fissi e timeout di 15 s: `GET /api/tailscale` restituisce installato, stato, nome della macchina, indirizzo HTTPS, se il Core è già pubblicato e se l'indirizzo è autorizzato. `POST /api/tailscale/connect` esegue `tailscale serve --bg <porta>`, legge il nome della macchina e lo aggiunge agli indirizzi ammessi (ADR-0137) senza file né riavvii; da lì gli inviti portano già il collegamento `jarvis://pair?…`.
- **Sicurezza:** solo proprietario (i dispositivi associati ricevono 403 come per ogni rotta non in elenco); leggere lo stato non autorizza nulla; **mai Funnel** (pubblico); l'host deve passare `valid_remote_host`; la pubblicazione è un gesto esplicito («Collega con Tailscale» in app macOS e Windows); un errore di Tailscale è mostrato com'è, senza cambiare gli indirizzi.
- **Non incluso:** installazione o accesso a Tailscale (restano dell'utente); scoperta automatica in LAN e QR.
- **Evidenza:** 2 test unitari (lettura del nome e dello stato da JSON sintetico, riconoscimento della porta pubblicata), test API (stato senza autorizzazioni), smoke E2E; Clippy e `cargo test --locked` verdi. Prova con Tailscale reale non eseguita (non installato qui).
- **Documenti interessati:** INTERFACE_AND_DEVICES.md, README.md, ROADMAP.md.

### ADR-0142 — Briefing settimanale della direzione

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** scelta del proprietario («implementa il briefing settimanale»).
- **Decisione:** nuova iniziativa proattiva (ADR-0097) `weekly_briefing` (attiva di default), giorno `weekly_day` (0 = lunedì … 6 = domenica) e orario `weekly_time` (08:30), con la stessa finestra di sei ore, le ore di silenzio e la deduplica per settimana ISO. Il testo è deterministico e locale, con tre parti: *Settimana trascorsa* (cose da fare completate/nuove/aperte, missioni concluse e in corso, esecuzioni di automazioni e fallimenti, ricordi aggiunti), *Richiede la tua attenzione* (ritardi, conferme in attesa, automazioni fallite, notifiche) e *Settimana in arrivo* (fino a cinque scadenze dei sette giorni successivi). Chiude rimandando ai calcoli aziendali (RICE, runway, margine). `POST /api/proactive/weekly` lo genera su richiesta.
- **Controlli:** nessun dato lascia il computer e non interroga i connettori; è una notifica di categoria `briefing`; `weekly_day` è validato (0–6) e `PATCH /api/proactive` rifiuta valori fuori formato.
- **Conseguenze:** il lunedì il briefing quotidiano e quello settimanale sono due notifiche distinte; il settimanale si disattiva dalle iniziative.
- **Evidenza:** test unitari del testo e della validazione, test di integrazione con orologio simulato (non scatta prima dell'orario, una sola volta a settimana, non negli altri giorni, giorno scelto, su richiesta), smoke E2E; Clippy e `cargo test --locked` verdi.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ROADMAP.md.

### ADR-0143 — Chat multiple

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout (Core, app macOS e Windows; Android usa la chat attiva) · **Fonte:** richiesta del proprietario («permettigli di poter avviare nuove chat ogni volta»).
- **Decisione:** tabella `conversations` e colonna `messages.conversation` (migrazione additiva: tutta la cronologia esistente resta nella chat `main`). Il Core tiene una chat attiva; messaggi, contesto del modello e cronologia riguardano solo quella. `GET/POST /api/conversations` (elenco, nuova), `POST /api/conversations/select`, `POST /api/conversations/delete`. Il titolo è il primo messaggio (48 caratteri), ricavato alla lettura e **mai salvato in chiaro**: la cifratura della chat resta intatta.
- **Controlli:** rotte riservate al proprietario; apertura ed eliminazione nel registro di audit; eliminare una chat rimuove solo i suoi messaggi (la principale viene svuotata); la memoria a lungo termine non dipende dalla chat.
- **Evidenza:** test API (chat separate, ritorno alla principale, 404, eliminazione) e smoke E2E.

### ADR-0144 — Lavori cloud in background

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («lavorare in background sul Cloud anche se l'app non è in primo piano»).
- **Decisione:** `POST /api/cloud/background` accetta lo stesso corpo della chat cloud e risponde subito `202` con un lavoro; l'esecuzione prosegue **nel Core** (non nell'app), quindi anche con l'app nascosta o chiusa. La risposta è scritta nella chat da cui è partito il lavoro anche se nel frattempo se ne è aperta un'altra (`conversation_id`, riservato al proprietario) e arriva una notifica (`category: mission`, anche push). `GET /api/cloud/jobs` elenca gli ultimi 30 lavori (solo in memoria; la risposta resta comunque nella chat). Al massimo quattro lavori in corso e due richieste cloud simultanee.
- **Controlli:** stessi controlli della chat cloud (credenziali rifiutate, policy, audit, conferme); un'azione che richiede conferma resta in attesa nella chat. Rotta riservata al proprietario. Il Core deve restare acceso: se si chiude il computer il lavoro si ferma.
- **Evidenza:** test API (202, completamento senza app, chat attiva intatta, risposta nella chat di partenza, notifica); nelle app: casella «In background» e avatar con satelliti (Windows).

### ADR-0145 — Memoria: ricordi fissati ed episodi di chat

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora anche la memoria e come viene salvata e i ricordi»).
- **Decisione:** (1) un ricordo si **fissa** (`POST /api/memory/{id}/pin`): non scade, compare per primo nel contesto del modello come «[importante]» (fino a 12) ed è mostrato come `pinned`; la marca sta nel campo `source` (nessuna migrazione). (2) Quando si apre una chat nuova, la chat appena chiusa lascia un **episodio** («Chat del …: N richieste, le ultime …») con scadenza come ogni episodio (`JARVIS_EPISODIC_DAYS`), così resta ritrovabile anche se la nuova chat parte vuota; non si registra se contiene credenziali o ha meno di due richieste.
- **Invariato:** ricerca ibrida BM25 + vettori locali, grafo, duplicati e reindicizzazione (ADR-0022); i ricordi con credenziali non entrano mai nel contesto.
- **Evidenza:** test API (fissare, scadenza assente, 404, liberare, episodio alla chiusura).

### ADR-0146 — Sviluppo di applicazioni, competenze e documento OpenAPI

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario (coding, «super applicazioni», nuove competenze, API, «addestra i modelli»).
- **Decisione:** nuovi ruoli del prompt (ADR-0136) `AppBuilder` (costruzione di applicazioni con interfaccia contemporanea: token di design, tema chiaro/scuro, responsive, accessibilità, movimento sobrio, test) e `CodeReview`; nove competenze incluse: Applicazione completa, Interfaccia moderna, Revisione del codice, Diagnosi dei difetti, Test del software, Progettazione di API, Basi di dati, Rilascio e infrastruttura, Sicurezza del codice (24 in tutto). Nuovo `GET /api/openapi.json` (OpenAPI 3.1, 50 rotte con accesso e formato degli errori) generato da una tabella unica; un test verifica che ogni rotta in sola lettura dichiarata esista.
- **«Addestramento dei modelli»:** i modelli sono servizi di terzi (cloud) o locali già addestrati; **il Core non può modificarne i pesi**. La specializzazione avviene nel contesto: ruoli, competenze, stile per famiglia di modello, feedback e profili di risposta (ADR-0095) già presenti. Un vero fine-tuning richiederebbe dataset, GPU e un provider che lo offra: resta una decisione separata.
- **Evidenza:** test dei ruoli, delle competenze con richieste naturali, del documento OpenAPI e delle rotte dichiarate.

### ADR-0147 — Docker: stato del collegamento e profili di risorse del sandbox

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora il collegamento a Docker e le sandbox per automazioni AI»).
- **Decisione:** `GET /api/docker` (solo proprietario) riunisce stato del motore (installato, avviato, permessi, versione, cosa fare), immagine pronta, limiti, sessioni e **profili**; `POST /api/docker/cleanup` rimuove i container del sandbox rimasti dopo un arresto brusco (audit `docker.cleanup`). Lo strumento `sandbox.run` accetta `profile`: `light` (256 MiB, 0,5 CPU, 64 processi, 10 s), `standard` (512 MiB, 1 CPU, 128 processi, 60 s, predefinito), `build` (1 GiB, 2 CPU, 256 processi) e `data` (1 GiB, 1 CPU). Cambiano solo i limiti: **per ogni profilo** restano nessuna rete, radice di sola lettura, nessuna capability, `no-new-privileges`, utente non root e solo la cartella della sessione montata.
- **Evidenza:** test degli argomenti `docker run` per tutti i profili (isolamento identico, limiti diversi), profilo sconosciuto rifiutato, test API dello stato; `docker run` reale non eseguito (Docker non disponibile qui).
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, descrittore `sandbox.run`.

### ADR-0148 — Personalità, forma di rivolgersi, conferme specifiche e rotte per compito

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario (personalità, «signore», «operazione completata», modelli per task).
- **Personalità (sostituisce il predefinito vuoto di ADR-0120):** la forma di rivolgersi predefinita è «Signore» (chiave assente); una chiave vuota salvata dal proprietario resta «nessun titolo». Nuove impostazioni `address_frequency` (`never`, `rare` predefinita, `often`) e `formal` (lei/tu, coerente per tutta la conversazione). Il prompt dice quando usare il titolo (nel saluto, nelle conferme importanti e nelle riserve, mai in ogni frase) e aggiunge l'ora del giorno (saluto adatto, di notte risposte più sobrie).
- **Conferme:** le risposte alle modifiche (personalità, profilo, iniziative, voce, sandbox, chat, ricordi fissati, indirizzi, Tailscale, pulizia Docker, briefing, lavori in background) portano un campo `message` che dice **cosa è successo** («Fatto, Signore. Ricordo fissato: lo terrò sempre presente…»), con aperture che variano e il titolo solo secondo la frequenza; nello stile sobrio è la sola frase. Il prompt vieta il generico «operazione completata». Le app mostrano il messaggio (egui: riquadro per 7 s; macOS: avviso) o, se manca, una frase contestuale.
- **Rotte per compito:** oltre a veloce, ragionamento, codice e contesto lungo, tre nuove rotte: `writing` (testi e riassunti), `translation` e `analysis` (dati e calcoli, modello di ragionamento), ciascuna con istruzioni e lunghezza di risposta proprie; scrittura e traduzione usano il modello veloce con il suo tempo massimo ridotto. Il codice ha sempre la precedenza.
- **Evidenza:** test della personalità (predefinito, titolo vuoto, frequenza, lei/tu), delle conferme, delle rotte per entrambi i provider e API.

### ADR-0149 — Protezione: notifiche cifrate, guardia anti-iniezione, punteggio e intestazioni

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora la protezione», «dashboard più crittografata»).
- **Cifratura a riposo:** oltre a messaggi e attività (ADR-0037), ora anche le **notifiche** sono cifrate (XChaCha20-Poly1305, scopo `notification:<id>`); le righe precedenti si migrano in modo idempotente all'avvio e restano leggibili durante la migrazione. Le colonne di ricerca (data, chiave di deduplica, stato di lettura) restano in chiaro.
- **Guardia anti-iniezione:** `guard.rs` cerca nei risultati degli strumenti (pagine web, file, connettori) segnali di dirottamento in italiano e inglese («ignora le istruzioni…», cambio di ruolo, richiesta del prompt, esfiltrazione, aggiramento delle conferme); se li trova antepone un avviso che li declassa a dato. Il contenuto non è alterato né troncato; policy e conferme restano comunque l'ultima difesa.
- **Punteggio:** `GET /api/security` aggiunge `encryption` (cifratura a riposo, algoritmo, cosa copre, segreti, audit firmato) e `posture` (punteggio 0–100, livello alta/media/bassa e controlli con il rimedio per ognuno). La Plancia (macOS e Windows) mostra la scheda «Protezione».
- **Intestazioni:** `X-Frame-Options: DENY`, `Permissions-Policy` (microfono solo same-origin), `Cross-Origin-Opener-Policy` e `Cross-Origin-Resource-Policy` same-origin, in aggiunta a CSP, `nosniff` e `no-store`.
- **Non incluso:** cifratura della memoria a lungo termine (serve il testo in chiaro per l'indice FTS, ADR-0022) e del file di audit (firmato, non cifrato).
- **Evidenza:** test di migrazione e lettura delle notifiche cifrate, test della guardia, test API del punteggio.

### ADR-0150 — Nuovi strumenti di analisi, ricerca ed esportazione delle chat, stop dei lavori

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («aggiungi altre funzioni», «migliora ancora tutto») e proposte in sospeso (pulsante di stop).
- **Strumenti (solo calcolo, nessun dato salvato):** `data.stats` (conteggio, media, mediana, minimo, massimo, quartili, deviazione standard, valori anomali con la regola dell'intervallo interquartile; da una lista o dai numeri di un testo, anche con la virgola decimale), `text.extract` (email, indirizzi web, date, hashtag, telefoni, senza duplicati) e `text.diff` (confronto riga per riga con sottosequenza comune, somiglianza in percentuale; fino a 400 righe). Disponibili ai modelli cloud come strumenti di calcolo, via MCP (ADR-0138) e offline («statistiche: 2, 4, 4, 5», «estrai email da: …»).
- **Chat:** `GET /api/conversations/search?q=` cerca nei messaggi di tutte le chat (ultimi 3000, maiuscole e accenti ignorati, estratto del contesto); `GET /api/conversations/export?id=` produce la chat in Markdown (le app la copiano negli appunti). I messaggi restano cifrati sul disco: la ricerca li decifra in memoria.
- **Stop:** `POST /api/cloud/jobs/{id}/cancel` ferma un lavoro in background in corso (404 se sconosciuto, 409 se già concluso, audit `cloud.job.cancel`); il risultato non viene scritto nella chat. La chiamata già in volo verso il provider non si interrompe, ma la sua risposta è scartata.
- **Evidenza:** test unitari dei tre strumenti, test API (ricerca, esportazione, 404/409 dello stop, strumenti via MCP), test Python delle regole offline e smoke E2E.
- **Documenti interessati:** AUTONOMY_AND_TOOLS.md, ROADMAP.md, `contracts/catalog/tools/`.

### ADR-0151 — Registro universale degli strumenti, salute, cronologia e arresto di emergenza

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout · **Fonte:** specifica «Advanced Tool System & Autonomous Intelligence Upgrade» del proprietario.
- **Analisi preliminare:** il catalogo (`contracts/catalog/tools`), la policy, i permessi del proprietario (ADR-0116), l'audit, gli agenti specializzati, le missioni, i connettori e il server MCP esistevano già e sono stati **riusati**; mancavano un registro normalizzato, la salute e la cronologia di esecuzione, un arresto di emergenza e un piano a grafo. Stack invariato (Rust, Python, SQLite).
- **Decisione:** `toolsys.rs` ricava per ogni descrittore categoria, piattaforme, disponibilità offline, reversibilità, dipendenze e requisiti (sovrascrivibili dal descrittore, nessuna modifica agli 81 file); `GET /api/tools/registry` (filtri `category`, `risk`, `platform`, `offline`, `q`), `GET /api/tools/health` e `/history`. Ogni esecuzione registra in `tool_runs` strumento, istante, durata, esito e codice d'errore, **mai argomenti né risultati**. Lo stato è `healthy`, `degraded`, `failing` o `unknown`. Il ciclo di vita «blocca/sblocca» riusa le regole del Centro permessi. **Arresto di emergenza** (`POST /api/emergency`): profilo di sola lettura imposto dalla policy (`policy.emergency_stop`), ferma lavori in background, piani, scheduler, missioni e iniziative; solo proprietario, con audit e notifica.
- **Non incluso:** installazione o rimozione di strumenti nativi, plugin dal disco, SDK pacchettizzato, client MCP (vedi IMPLEMENTATION_STATUS.md).
- **Evidenza:** test unitari dei campi derivati e dello stato di salute; test API di registro, filtri, salute, cronologia e arresto di emergenza.

### ADR-0152 — Piani di esecuzione

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** un piano è un grafo di passi (strumenti del catalogo) con dipendenze, riferimenti tipizzati fra passi (`${passo.campo}`), ripetizioni (≤ 2), `on_error`, operazione inversa e budget di tempo e di passi (≤ 20 passi). Si esegue un passo alla volta fuori dal blocco del Core, con **checkpoint cifrato** dopo ogni passo; ogni strumento passa da policy, regole del proprietario, arresto di emergenza e audit con soggetto `agent:planner`. I passi che richiedono conferma mettono il piano in `waiting` finché il proprietario non li approva (`/approve`). Annullamento, **rollback** dei passi con `undo` (con `confirm` per quelli che richiedono conferma), **replay**, idempotenza per chiave, ripresa dopo la chiusura del Core (`interrupted`). Notifica alla conclusione o all'attesa.
- **Non incluso:** esecuzione parallela, scomposizione automatica dell'obiettivo, ripianificazione, validazione indipendente (vedi roadmap fasi A–C).
- **Evidenza:** test unitari di validazione, ordine, riferimenti e blocco dei dipendenti; test API di esecuzione con valori passati fra passi, fallimenti e ripetizioni, conferma, rollback, replay e annullamento.

### ADR-0153 — Registro degli agenti e spiegazione dell'instradamento

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** `GET /api/agents` descrive i sette specialisti e il generalista (competenze dagli strumenti, rischio massimo, contesto isolato o condiviso, risorse); `GET /api/agents/route?q=` mostra quale agente riceverebbe una richiesta e per quali parole chiave; `GET /api/models/route?q=` mostra la rotta per compito, il modello per provider, il ripiego e la riservatezza (credenziali bloccate, dati personali rilevati, profilo condiviso) **senza inviare nulla**. Non si introducono nuovi agenti: si riusano quelli esistenti.
- **Evidenza:** test API del registro e dell'instradamento.

### ADR-0183 — «Voce non disponibile»: causa e riconoscimento del Mac

- **Data:** 2026-11-25 · **Stato:** corretto nel Core (verificato); **Swift verificato solo dalla CI** (nessun compilatore qui, mai provato con un microfono).
- **Causa (riprodotta):** dal commit «Usa solo inferenza cloud…» il Core avviava il worker con `JARVIS_VOICE_ENGINE=off` **sempre**, quindi `POST /api/voice/wake` e `/transcribe` rispondevano `voce disattivata (JARVIS_VOICE_ENGINE=off)` e l'app macOS mostrava «Voce non disponibile» a ogni avvio, anche con i pacchetti vocali installati. Questo contraddiceva ADR-0010 e ADR-0054 («il setup installa la voce»). In più `POST /api/setup/start` rifiuta l'installazione con «La voce richiede un provider audio cloud», che non esiste.
- **Correzione nel Core:** il worker riceve `JARVIS_VOICE_ENGINE` solo se lo sceglie il proprietario o le verifiche (`auto`, `local`, `fake`, `off`); senza scelta usa `auto`: i motori installati, altrimenti un messaggio che dice cosa manca («STT faster-whisper non installato (./jarvis.sh voice-setup); TTS Piper non installato…»).
- **App macOS:** l'ascolto «Hey Jarvis» usa ora, come primo motore, il **riconoscimento vocale del Mac sul dispositivo** (`SFSpeechRecognizer` italiano, `requiresOnDeviceRecognition`): nessun modello da installare, nessun audio fuori dal computer, nessun file. Se mancano il permesso, la lingua o la dettatura sul dispositivo, l'app dice cosa fare e ripiega sul motore del Core. L'etichetta generica «Voce non disponibile» diventa «Ascolto non disponibile» con il motivo accanto. Nuova voce `NSSpeechRecognitionUsageDescription` nell'`Info.plist`.
- **Compromesso di riservatezza da confermare:** ADR-0134 chiede che l'app ascolti «solo Hey Jarvis, nient'altro»: il motore del Core risponde solo sì/no e non produce testo. Il riconoscimento del Mac invece **trascrive in memoria** ciò che si dice mentre è in ascolto (sul dispositivo, per una sessione di circa un minuto, senza salvare né inviare) e agisce solo dopo la parola di attivazione. L'app lo dichiara nella scheda «Voce di JARVIS». Chi vuole la garanzia più stretta usa il motore del Core (`./jarvis.sh voice-setup`).
- **Non incluso:** installazione dei modelli vocali dall'app (resta `voice-setup`; il rifiuto «provider audio cloud» di `/api/setup/start` non è stato toccato), la stessa correzione per Windows (ADR-0140) e Android.
- **Evidenza:** riprodotto e verificato con un Core avviato: prima `voce disattivata (JARVIS_VOICE_ENGINE=off)`, dopo il messaggio vero sui motori mancanti. **Sospeso:** compilazione dello Swift (CI) e prova con microfono e dettatura italiana sul Mac.

### ADR-0182 — Client MCP più robusto, Hugging Face con un gesto e memoria portabile

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Client MCP (`mcp_client.rs`):** (1) **sessione riusata** per 10 minuti (una `initialize` ogni dieci richieste, non una per richiesta) e **riaperta da sola** se il server la dimentica (HTTP 404 o messaggio di sessione), con una sola ripetizione; (2) **elenco strumenti a pagine** (`nextCursor`, fino a 5 pagine e 200 strumenti), in cache 5 minuti, con **impronta** e segnalazione `changed` quando uno strumento compare, sparisce o cambia schema; (3) **nuovi tentativi limitati**: HTTP 429/502/503/504 sempre (con `Retry-After` fino a 5 s), guasto di rete solo per le letture, **mai per `tools/call`** (potrebbe aver avuto effetto), al più 3 tentativi e 40 s in tutto; (4) **argomenti verificati sullo schema del server** (obbligatori, tipo, valori ammessi) prima di partire e strumento sconosciuto rifiutato; (5) risposte con testo, risorse testuali, collegamenti e contenuto strutturato, **immagini e audio ridotti a un segnaposto** con tipo e dimensione; (6) **statistiche per server** (chiamate, errori, ultimo esito, latenza) in `GET /api/mcp/servers` e `POST /api/mcp/servers/health` (raggiungibilità, latenza, strumenti, strumenti di sola lettura, cambiamenti).
- **Politica per strumento, decisa dal proprietario** (`POST /api/mcp/servers/policy`): `allow` (strumenti chiamabili; vuoto = tutti; vale per `mcp.list`/`mcp.call`/`mcp.read`, errore `mcp.not_allowed`) e `auto` (strumenti che JARVIS può eseguire **senza conferma**). `auto` è accettato solo per strumenti che il server dichiara di sola lettura nell'elenco già letto o, per un server noto, nel catalogo; senza l'elenco non si può approvare nulla. Si usa con il nuovo strumento **`mcp.read`** (rischio medio, effetto «lettura», nessuna conferma): rifiuta (`mcp.not_auto`) tutto ciò che non è in `auto`. **`mcp.call` resta sempre con conferma** (rischio alto): niente scorciatoie nella policy né nel contratto.
- **Hugging Face:** `GET /api/mcp/presets` elenca i server noti; `POST /api/mcp/servers` con `{"preset": "huggingface"}` (e, facoltativo, `"auto_suggested": true`) autorizza `https://huggingface.co/mcp` con gli strumenti di sola lettura consigliati dal catalogo (`hub_repo_search`, `hub_repo_details`, `hf_fs`, osservati il 2026-11-25 insieme a `hf_whoami`, tutti con `readOnlyHint`). Nessuno strumento automatico di fabbrica. Nuova competenza **«Modelli e dataset su Hugging Face»** (25 competenze) e l'agente di ricerca riceve `mcp.list`, `mcp.read`, `mcp.call` e le parole «Hugging Face» / «modelli open source». Resta valido ADR-0166: nessun provider Hugging Face dentro il Core; gli argomenti partono verso Hugging Face e il risultato è un dato non attendibile.
- **Memoria:** `GET /api/memory/export` (documento `jarvis.memory-export/1` senza identificativi e senza credenziali), `POST /api/memory/import` (anteprima o scrittura: valida, salta i doppioni, rifiuta credenziali e voci malformate con il motivo; «verified» arriva come «stated», origine ignota come «uncertain», provenienza `import`), `POST /api/memory/contradictions/resolve` (tiene la voce scelta, la marca «verified», elimina le altre della coppia; rifiuta se una è fissata), `POST /api/memory/compact` (riassume per settimana gli episodi più vecchi di N giorni in un solo riassunto «dedotto» che dura 180 giorni, mai i fissati; anteprima di default) e **scadenza adattiva**: un episodio davvero usato (importanza almeno 0,3) vive almeno 90 giorni. Tutto con audit.
- **Non incluso:** OAuth e flussi SSE a lunga durata per MCP, risorse e prompt MCP, provider Hugging Face nel Core, il comando in chat «ricorda per il progetto X» (si usa l'API o l'argomento `project` dello strumento), esecuzione dei modelli trovati.
- **Evidenza:** 14 test unitari del client (sessione riusata e riaperta, pagine e impronta, tentativi, validazione, contenuti, politica, salute, server noti) con un server HTTP locale, 1 d'integrazione con il Core (mcp.read prima e dopo l'approvazione, elenco ammesso, preset, salute), 4 unitari e 1 d'integrazione della memoria, e il controllo del routing verso la ricerca. Lo schema reale degli strumenti di Hugging Face è stato letto con `curl` (il client Rust non raggiunge host esterni da questo ambiente). **Sospeso:** prova del client Rust contro il server reale di Hugging Face.

### ADR-0181 — Plancia dal mock-up: schede, ritratto e avatar che parla e cambia colore

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout; **Swift verificato solo dalla CI** (nessun compilatore Swift qui, resa a schermo non vista).
- **Decisione:** la Plancia macOS segue la struttura del mock-up del proprietario: saluto «Ciao, sono JARVIS» con onda e barra dei comandi a sinistra; **avatar al centro**; **Agenti attivi**, **Meteo** e **Progetti in corso** a destra; sotto cinque schede — **Stato sistema** (anelli CPU/RAM/SSD), **Attività di oggi**, **Notifiche**, **Media / Controllo**, **Monitoraggio** (grafico del carico e contatori). Le vecchie schede (categorie, riquadri, attività recenti, missioni) restano in «Altre sezioni e dettagli». Il Dock del mock-up è del sistema operativo e non si replica. **Dati veri:** `GET /api/dashboard/summary` (specialisti con stato, piani come progetti con avanzamento, cose da fare, notifiche, tempo di attività, operazioni, esito degli strumenti) e il carico del sistema campionato ogni 5 secondi; niente cifre inventate (dove un dato non esiste — temperatura, rete — la scheda non lo mostra). **Meteo facoltativo:** nessuna città predefinita nel codice né nel repository; la scheda chiede una città, che esce dal computer verso Open-Meteo solo dopo la scelta (`PATCH /api/dashboard`, in cache 30 minuti, `GET /api/dashboard/weather`). **Media facoltativo:** legge e comanda Musica o Spotify con AppleScript solo dopo il consenso e se sono già aperti; macOS chiede a sua volta l'autorizzazione. **Avatar:** colore per stato (ciano a riposo, ciano caldo mentre parla, verde acqua in ascolto, viola mentre pensa, rosso in allerta, sommato alla tonalità scelta), **alone che pulsa con la voce** (sillabe sintetiche; in ascolto segue il microfono), e **ritratto dell'utente**: se esiste `~/Library/Application Support/JARVIS/avatar.png` la Plancia mostra quell'immagine con alone, anello a barre che reagisce alla voce e anello luminoso rotante; senza, resta il disegno olografico. Con «Riduci movimento» gli alberi di animazione si fermano.
- **Non incluso:** il volto fotorealistico del mock-up **non è nel repository** (non posso generare immagini; va fornito dall'utente come `avatar.png`, con i diritti d'uso); sincronia delle labbra sul ritratto; temperatura e traffico di rete; la stessa Plancia per Windows (egui) e Android; ampiezza reale della sintesi vocale (oggi l'alone usa un ritmo sintetico).
- **Evidenza:** 3 test unitari (località opzionale e validata, avanzamento, vista del meteo) e 1 d'integrazione sul riepilogo con piani, specialisti e contatori reali. **Sospeso:** compilazione e resa dello Swift (solo CI su macOS), prova con Musica/Spotify e con un ritratto.

### ADR-0180 — Più dispositivi: passaggio di attività e livelli per nodo

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** (1) **Passaggio di attività** (`handoff.rs`, tabella `handoffs`): il proprietario crea dal computer del Core un pacchetto di ripresa (`POST /api/handoffs`: destinazione `device_id` o `any`, obiettivo o `plan_id`, nota) con l'obiettivo e lo stato del piano; un dispositivo associato con permesso `chat` vede quelli a lui destinati (`GET /api/handoffs`) e li riprende (`POST /api/handoffs/{id}/accept`). **Nessun permesso viene trasferito**: chi riprende lavora con i propri permessi, livelli e conferme. **Una sola ripresa:** il secondo dispositivo riceve «già ripreso da …» (risoluzione del conflitto: vince il primo, in modo esplicito); scadenza 30 minuti; al massimo 20 aperti; testo ripulito e rifiutato se contiene credenziali. Notifica di promemoria e audit (`handoff.create` dal proprietario, `handoff.accept` con il dispositivo come attore). (2) **Livelli di autonomia per nodo:** `autonomy.json` accetta `devices` (solo dispositivi associati e non revocati); l'assegnazione di uno strumento a un nodo applica il livello (minimo fra generale e suo): livello 0–1 nega le modifiche, livello 2 le sottopone a conferma (`node.autonomy`, `node.not_confirmed`).
- **Correzione a un difetto preesistente:** la notifica di fine piano usava la categoria `automation` senza `workflow_id` e la vista `plans`, entrambe non valide per il contratto: veniva scartata in silenzio (`let _ =`). Ora usa la categoria `system` e la vista `tasks`.
- **Non incluso:** sincronizzazione offline con coda di modifiche e fusione dei conflitti (oggi la cronologia e le notifiche si rileggono con `/api/chat/sync`), livello di autonomia per i dispositivi che non sono nodi (i loro permessi restano quelli per rotta), esecuzione remota di un piano su un altro dispositivo, interfaccia nelle app.
- **Evidenza:** 3 test unitari del passaggio (una sola ripresa, scadenza, segreti, `any`), 1 del livello per dispositivo, 1 d'integrazione (obiettivo e piano nel pacchetto, destinatario, secondo dispositivo rifiutato, audit). **Sospeso:** prova con dispositivi reali e con un nodo vero.

### ADR-0179 — Autocorrezione, riflessione e prove osservabili

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** `reflect.rs`. **Classificazione dei guasti** da codice e messaggio: `authorization` (policy, vincolo, livello), `scope` (fuori workspace), `transient` (tempo scaduto, servizio occupato), `precondition` (risorsa assente), `input` (argomenti o riferimenti errati), `verification` (prova mancante), `internal`, `unknown`; ognuna con causa probabile e strategie alternative. Ogni passo fallito di un piano porta `class`, `retryable`, `cause` e `strategies`. **Nuovi tentativi controllati:** un passo con `retries` non ripete quando la classe dice che riprovare è inutile (rifiuto della policy, percorso fuori workspace, argomento errato, prova mancante); una risorsa assente (`precondition`) si può ancora riprovare, perché può comparire per opera di un altro passo (comportamento già coperto da `plan_failures_retries_and_bad_definitions_are_handled`, rimasto invariato); i guasti passeggeri e sconosciuti continuano a ripetere nei limiti già stabiliti (2 per passo, budget di tempo e di passi, ripianificazione al massimo 3 volte). **Prove osservabili:** dopo `fs.write`, `fs.mkdir`, `fs.delete`, `fs.move` e `memory.remember` il Core rilegge lo stato (il file esiste con la dimensione scritta, la cartella c'è, l'elemento non c'è più, il ricordo è salvato); se la prova manca il passo fallisce con `plan.unverified`: non basta che lo strumento o il modello dichiarino di aver finito. **Diagnosi:** `GET /api/plans/{id}/diagnosis` elenca i passi falliti con classe, causa, strategie, passi bloccati e una raccomandazione (nessun guasto, riprova, ripianifica con strategia diversa, limite raggiunto). **Contraddizioni:** `GET /api/memory/contradictions` trova nel grafo di memoria la stessa coppia soggetto-relazione con oggetti diversi («il mio cane si chiama Fido» e «… Rex»).
- **Non incluso:** strategia alternativa scelta e avviata automaticamente dal Core (la ripianificazione resta una proposta da approvare), prove per gli strumenti che non hanno un controllo semplice (rete, connettori, terminale), analisi della causa oltre le classi, contraddizioni fra fonti diverse dalla memoria.
- **Evidenza:** 4 test unitari (classi, annotazione, prove di scrittura/eliminazione/ricordo, contraddizioni) e 2 d'integrazione (nessun nuovo tentativo su percorso fuori workspace, diagnosi con passi bloccati e raccomandazione; scrittura verificata e contraddizione nella memoria).

### ADR-0178 — Memoria cognitiva: importanza, affidabilità, progetto e consolidamento

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** `memcog.rs` aggiunge alla memoria esistente (ADR-0022/0037/0125) cinque colonne (`importance`, `reliability`, `uses`, `last_used_at`, `project`) con migrazione additiva e tabella `memory_ops`. **Importanza** 0–1 deterministica e spiegabile: base per tipo (preferenze 0,7, fatti 0,5, episodi 0,3), +0,25 se fissato, +0,02 per uso (fino a 10), meno l'età (al massimo 0,2, non per fissati e preferenze), più o meno l'affidabilità; entra nel ricalcolo della ricerca come **ordinamento**, mai come soglia che nasconda un ricordo. **Affidabilità:** `verified` (solo per decisione del proprietario: lo strumento di memoria lo rifiuta), `stated`, `inferred` (gli episodi lo diventano da soli), `uncertain`. **Uso:** entrano nel conteggio solo i ricordi trovati dalla richiesta, non quelli sempre nel contesto. **Memoria di progetto:** un ricordo con `project` entra nel contesto solo se la richiesta cita quel progetto; `memory.remember` accetta `project` e `reliability`. **Consolidamento:** ricalcola le importanze e, per i quasi-doppioni (similarità ≥ 0,93, stesso tipo), tiene il più importante (a parità il più vecchio), ne somma gli usi ed elimina l'altro; **mai** un ricordo fissato; anteprima di default, applicazione con audit e registro in `memory_ops`. API: `GET /api/memory/cognitive` (`kind`, `project`), `POST /api/memory/{id}/meta` (affidabilità e progetto), `POST /api/memory/consolidate` (`apply`). La difesa di prima resta: la memoria rifiuta le credenziali e il contesto le filtra.
- **Non incluso:** scadenza adattiva dall'importanza (gli episodi mantengono i 30 giorni), consolidamento che riassume più episodi in uno, correzione automatica dell'affidabilità da feedback, interfaccia nelle app, grafo personale oltre alle triple già estratte.
- **Evidenza:** 4 test unitari (pesi dell'importanza, ricalcolo e usi, validazione e filtri, anteprima e fusione senza toccare i fissati) e 1 d'integrazione (progetto fuori contesto, affidabilità, strumento con progetto, consolidamento con audit).

### ADR-0177 — Collaborazione fra specialisti

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** una richiesta che si divide in 2–4 parti (` e poi `, ` dopo `, `;`…) con **almeno due specialisti diversi** (sistema, azienda, codice, memoria, ricerca, sicurezza, automazioni) è svolta da `orchestrator/collab.rs`: ogni parte va allo specialista giusto con i soli strumenti del suo ambito, **in ordine e una alla volta**. Le parti condividono un **contesto** (gli esiti di quelle già fatte, passati come dati e non come istruzioni, `core.shared`), nessuna scrittura avviene in parallelo (conflitti evitati per sequenza), uno specialista che fallisce **cede la parte al generalista** (ripiego, dichiarato nella risposta) e il risultato è una sola risposta con l'esito di ciascuna parte (`**1. system** — …`, passi `collaboration` con agente richiesto, agente usato, ripiego e stato). Ogni parte passa da policy, conferme, livelli di autonomia, supervisione e audit come una richiesta a sé. Se una parte chiede una conferma le successive non partono e la risposta le elenca come non avviate. Una richiesta con un solo specialista o con nessuno, o con più di 4 parti, resta al generalista come prima.
- **Non incluso:** esecuzione **parallela** di agenti (un solo Core con un solo blocco di stato; il parallelo esiste solo fra gli strumenti di un piano), ripresa automatica delle parti non avviate dopo una conferma, parti che dipendono davvero dal risultato delle precedenti in modo strutturato (oggi lo vede il modello nel contesto condiviso), risoluzione dei conflitti oltre la sequenza.
- **Evidenza:** test d'integrazione con sistema e memoria (parti in ordine, risposta aggregata, ricordo davvero salvato) e controllo che una frase con un solo specialista non attivi la collaborazione.

### ADR-0176 — Pausa, vincoli in corso e prova a secco dei piani

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** i piani si **sospendono** (`paused`) senza annullarli: `POST /api/plans/{id}/pause`, riprendono con `/run`; il pianificatore si ferma al passo successivo, il budget di tempo riparte alla ripresa. I **vincoli** (`POST /api/plans/{id}/constraints`) si possono cambiare mentre il piano esiste: `read_only`, `forbid_tools` (con prefissi), `forbid_effects`, `max_risk`. Sono solo restrizioni: prima di ogni passo il Core salta (`plan.constraint`) quelli che li violano, e i dipendenti seguono; valgono per ogni strumento e agente del piano perché l'applicazione è nel pianificatore e non nei singoli agenti. `GET /api/plans/{id}/dry-run` fa la **prova a secco**: per ogni passo la decisione della policy (con profili, regole e livelli di autonomia), la conferma richiesta, la reversibilità, i vincoli e l'uso di risultati precedenti, **senza eseguire nulla né scrivere decisioni nell'audit**. Da chat: «aspetta/fermati/stop/basta» **sospendono**; «annulla/lascia stare» **annullano**; «riprendi/continua/vai avanti» riprendono un piano sospeso (con «senza modificare/scrivere/cancellare» aggiungono `read_only`); «prima fammi vedere cosa vuoi fare/simula» mostra la prova a secco. Senza un piano interessato la frase prosegue come chat normale. La ripresa da chat affida il piano al pianificatore anche nella chat in streaming. Modifica ADR-0173: «fermati» ora sospende invece di annullare.
- **Non incluso:** il vincolo «senza toccare il database» non ha uno strumento di database da riconoscere (oggi equivale a sola lettura); vincoli per singolo passo; sospensione di attività che non sono piani (missioni, ricerche cloud in background); prova a secco con gli argomenti già risolti dei passi che usano risultati precedenti.
- **Evidenza:** test unitario dei vincoli; 3 d'integrazione (pausa/ripresa in sola lettura con passi saltati e dipendenti, validazione e distinzione fra sospendere e annullare, prova a secco senza effetti anche da chat). **Sospeso:** interfaccia nelle app (oggi API e chat).

### ADR-0175 — Stato del mondo

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** modulo `world.rs` con tre tabelle (`world_entities`, `world_edges`, `world_events`, nello stesso database). Ogni elemento (sistema, disco, processo, repository, piano, missione, nodo, automazione, strumento, regole di sicurezza, fatto) dichiara la **fonte** — `observed` (letta dal Core) o `inferred` (dedotta, con fiducia e motivo) —, una **versione** che cresce solo quando il valore cambia davvero e una **durata di validità**: oltre quella è `stale` e non va presentato come attuale. **Un'osservazione ancora valida non è mai sostituita da una deduzione** (rifiutata); un'osservazione nuova sostituisce la deduzione e, se i valori divergono, registra una **contraddizione** (altrimenti una conferma). Gli elementi spariti dall'ambiente osservato sono rimossi con un evento `disappeared`, insieme ai loro archi. Archi di dipendenza (`piano → strumento`, `nodo → strumento`), cronologia limitata a 2000 eventi, al massimo 2000 elementi, valori di al più 2 KiB. Solo nomi, stati e conteggi: mai contenuti di file, righe di comando né segreti (una deduzione con una credenziale nel motivo è rifiutata). `POST /api/world/sync` rilegge le fonti reali (statistiche di sistema, dischi, 10 processi, repository della workspace se è Git, piani, missioni, nodi, automazioni, salute degli strumenti, profilo, arresto di emergenza e livello di autonomia); `GET /api/world` (filtri `kind`, `stale`), `/api/world/events`, `/api/world/dependencies`, `POST /api/world/infer` (deduzioni, solo proprietario).
- **Non incluso:** sincronizzazione periodica automatica (oggi su richiesta), stato di servizi del sistema operativo e di dispositivi non nodi, uso dello stato nel contesto della chat, interfaccia nelle app.
- **Evidenza:** 7 test unitari (versioni, osservato contro dedotto, contraddizioni, dato vecchio, scomparsa, dipendenze, limiti e segreti) e 1 d'integrazione con le fonti reali.

### ADR-0174 — Livelli di autonomia

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** `Rules::verdict_for(strumento, effetti, rischio, agente)` aggiunge ai profili e alle regole di ADR-0116 i **livelli 0–5** (`<dati>/autonomy.json`): 0 nessuno strumento; 1 solo letture; 2 ogni modifica con conferma; 3 comportamento del catalogo senza attività in background; 4 (predefinito) come 3 con missioni, iniziative proattive e osservazioni di file; 5 riservato, oggi come 4. Configurabili per agente, strumento (con prefisso) e categoria di rischio; vale il **livello minimo** fra quelli applicabili, e il verdetto finale è il più severo fra profilo, regole e livello: **un livello non allarga mai nulla**, in continuità con l'invariante di ADR-0116. La policy lo applica in modo deterministico per ogni chiamata (soggetto = agente che propone); le attività che partono da sole (`mission_due`, `proactive_tick`, osservazioni in `run_due`) si fermano sotto il livello 4. `GET/PATCH /api/autonomy` solo dal computer del Core (i dispositivi ricevono 403), non da chat: due frasi ostili in chat non cambiano nulla (test). File illeggibile: livello 2 (fallire chiuso). Ogni modifica è nell'audit (`autonomy.update`).
- **Non incluso:** livelli per dispositivo e per singola operazione (i dispositivi hanno già i permessi per rotta; manca il livello per dispositivo), il significato proprio del livello 5, interfaccia nelle app (si usa l'API), motivo di rifiuto distinto nel registro (oggi `policy.owner_rule_deny`/`owner_rule_ask`).
- **Evidenza:** 3 test unitari (minimo fra livelli, nessun allentamento di profili e regole, validazione e file corrotto) e 2 d'integrazione (livello 0 e 1 con la chat, missioni ferme sotto il 4, nessuna modifica da chat). **Sospeso:** interfaccia grafica.

### ADR-0173 — Motore di intento e di contesto

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (non committata, su richiesta del proprietario).
- **Decisione:** nuovo modulo `intent.rs`, deterministico, locale e senza modello. Per ogni frase ricava il **tipo** (domanda, istruzione, osservazione, autorizzazione, annullamento, correzione, cortesia), gli **ambiti** coinvolti con punteggio (`agents::rank`), il **multi-intento** (frasi divise da «e poi», «dopo», «quindi»…), le **entità** (numeri, percentuali, durate, file, indirizzi), i **riferimenti** («fallo partire», «sistemalo», «continua», «come prima», «questo/quello») risolti con il contesto che il Core ha davvero osservato (conversazione recente, piani con il loro stato, file citati, errori), la **fiducia** e, se serve, una **domanda di chiarimento**. Ogni riferimento dice da dove viene la risoluzione e l'età del dato; oltre 24 ore è `stale` e l'indicazione al modello lo segnala come non attuale. «Come prima» rimanda alla memoria (procedure ed episodi) invece di inventare. **Non concede mai nulla**: `grants_permission` è sempre falso; un'osservazione («non funziona un cazzo») porta al più a una diagnosi in sola lettura, un assenso generico («vai») vale solo per una conferma già proposta dal Core. L'analisi entra nel prompt come indicazione del Core nella stessa via delle competenze (`core.intent`, senza strumenti), si legge con `POST /api/intent/analyze` e, nel caso sicuro per eccellenza, **«aspetta, fermati» / «lascia stare» fermano i piani in corso** (dal ADR-0176: «aspetta/fermati» sospendono, «annulla/lascia stare» annullano) (fermare non allarga nulla; i passi già fatti restano nel registro).
- **Non incluso:** classificazione appresa o con modello per i casi dubbi, applicazione/finestra attiva, repository e schermo nel contesto automatico (esistono come strumenti con conferma), pausa vera di un piano, apprendimento delle correzioni.
- **Valutazione:** due insiemi separati (`tests/fixtures/intent_dev.tsv`, `intent_validation.tsv`). Sviluppo 39/39; **validazione 40/46 (87%)** con frasi scritte da me, non dal proprietario: indipendenza parziale. Errori noti documentati in `VALIDATION_REPORT.md` e non corretti guardando la validazione.
- **Evidenza:** 7 test unitari delle regole, test d'integrazione sull'annullamento dei piani e sulla risoluzione con piani reali. **Sospeso:** comportamento del modello sull'indicazione; prova con contesto reale d'uso.

### ADR-0172 — Richieste di delega: prima un piano da approvare

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** una frase breve di delega o troppo ampia («sistema tutto», «aggiorna tutto», «metti/mettilo online», «fai tu», «occupatene tu», «pensaci tu», «risolvi tutto»; massimo 8 parole, riconoscimento esatto) non avvia più azioni a caso: **con un modello cloud configurato** il Core chiede il piano al pianificatore (ADR-0160) con un obiettivo che parte da strumenti di sola lettura e lascia le modifiche da confermare, salva una **bozza** (`pending`, mai avviata) e risponde in chat con numero di passi, rischio massimo e l'elenco; la bozza si rivede e si avvia da «Piani», dove ogni passo rischioso chiede comunque conferma. Vale per `/api/chat` e `/api/chat/stream` del proprietario (non per i dispositivi). **Senza modello, o se la proposta fallisce, la chat prosegue come prima** (nessun errore nuovo). Audit e validazione sono quelli del pianificatore (`plan.propose`); nessuno strumento è eseguito dalla proposta.
- **Non incluso:** avvio automatico dopo l'approvazione dalla chat, riconoscimento di frasi di delega più lunghe o composte, la stessa funzione per gli utenti (chat senza strumenti).
- **Evidenza:** test del riconoscimento (frasi sì/no), test HTTP del ripiego senza modello su entrambe le rotte (nessun piano creato). **Sospeso:** prova del percorso con un modello cloud reale (nessuna chiave qui); l'aspetto della risposta nelle app.

### ADR-0171 — Valutazione dell'instradamento e robustezza degli strumenti

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** l'instradamento verso gli agenti (parole chiave, ADR-0038) si **misura** con un corpus sintetico di 65 richieste italiane con l'agente atteso (`core/server/tests/routing_eval.rs`): accuratezza, elenco degli errori, nessun errore ammesso. La prima misura ha trovato due buchi (spazio libero sul disco, versione del sistema operativo) e un falso positivo evitato («disco volante»); parole chiave aggiunte: «spazio libero», «disco rigido», «sistema operativo», «uso della memoria», «batteria». Una frase come «controlla lo spazio sul disco» dentro una missione resta al generalista (serve agli strumenti delle missioni). Gli strumenti puri (21) sono provati con 23 argomenti ostili (tipi sbagliati, estremi, enormi, Unicode) senza panico (`tests/tools_robustness.rs`).
- **Corpus del proprietario e secondo stadio (stesso ADR):** 101 richieste reali etichettate dal proprietario (`tests/fixtures/routing_corpus.tsv`; `a/b` = più risposte valide, usato solo per 5 frasi davvero ambigue). Misura iniziale: **19/101** (quasi tutti al generalista, 3 a uno specialista sbagliato). Dopo le parole chiave (esclusive, invariate) c'è un **secondo stadio a radici di parola**: solo se nessuna parola chiave ha colpito, ogni ambito somma i punti delle radici (1; `+` = 3 per i segnali forti; `$` = parola intera) e vince chi è in testa di almeno un punto, altrimenti generalista. Guardie: missioni, nomi di file (`bozza.txt`) e «disco volante» restano al generalista; «avvisami/avvertimi/notificami» → automazioni; «processore» non è un processo; «api key», «porte aperte» → sicurezza. Esito: **101/101** (soglia di test 95%). Poiché le radici sono state scritte guardando il corpus, il 100% **sovrastima** la resa su richieste nuove. Specialisti con strumenti più utili (app e finestre, git, log, dipendenze, missioni, web e documenti per l'azienda, note per la ricerca), solo di rischio basso o medio già concessi altrove; nessuno strumento ad alto rischio in più.
- **Frasi ambigue (stesso ADR):** il prompt di base ora dice di **risolvere i riferimenti dal contesto prima di chiedere** (turni, memoria, progetto in uso, strumenti in sola lettura) e di chiedere, con le opzioni trovate, solo se il riferimento resta non identificabile; frasi colloquiali come «non funziona», «tieni d'occhio», «fai come l'altra volta», «aggiorna/metti online/sistema tutto», «fai tu» sono richieste operative con un comportamento atteso (diagnosi, monitoraggio con oggetto-frequenza-condizione, recupero della procedura, analisi di impatto e piano, delega entro policy e conferme). L'instradamento manda «tieni d'occhio» alle automazioni e «cosa di ieri»/«l'altra volta» alla memoria; le richieste che toccano più ambiti restano al generalista, che fa da orchestratore. Dieci casi del proprietario nel corpus con più risposte valide. **Non verificato:** il comportamento del modello (la risoluzione dal contesto dipende dal modello e dagli strumenti, non dalle regole).
- **Non è addestramento:** il Core non contiene modelli addestrabili (ADR-0146); «addestrare» qui significa mettere a punto regole con evidenze misurate. Il corpus è piccolo e scritto a mano: non prova la qualità su richieste reali.
- **Evidenza:** `cargo test --locked` completo, fmt e Clippy.

### ADR-0170 — Interfacce degli utenti: accesso, chat, profilo e pagina Utenti

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** nel client **egui** (Windows e macOS) la schermata «Collega JARVIS» ha tre schede: *Codice del dispositivo* (invariata), *Accedi* e *Registrati* (nome utente, password, come vuoi essere chiamato, codice di invito). La sessione si conserva in un file privato (`remote-user-token`, permessi 0600) accanto all'indirizzo del Core e si riprende all'avvio; accedere come utente toglie il token di un dispositivo e viceversa (vale l'ultimo accesso). In modalità utente l'app mostra solo **Chat** (conversazione personale con il Markdown leggero di ADR-0168, cronologia ricaricata, messaggio restituito nella casella se il Core lo rifiuta) e **Profilo**: riepilogo di «come ti parla JARVIS», modulo della personalità (nome, come ti chiamo, tu/lei, stile, tono, livello, ruolo, interessi, obiettivi), ricordi con «Dimentica», cancellazione della cronologia, cambio password (la sessione si rinnova), uscita ed eliminazione dell'account con conferma. Niente flusso di notifiche, niente cronologia né endpoint del proprietario. **L'avatar prende il colore del tono scelto** (formale, cordiale, diretto). Per il proprietario, nel client egui e nella console **SwiftUI** (che si collega solo al Core locale e quindi non ha accesso utente) c'è la pagina **Utenti**: chi può registrarsi, uso del modello cloud, creazione di inviti (il codice si vede una volta, con «Copia»), elenco con conteggi e ultimo accesso, disattiva/riattiva ed elimina con conferma. Il client controlla solo l'origine (HTTPS, o HTTP solo sul computer stesso) e cosa mostrare; password, inviti, limiti e isolamento restano nel Core. Le sessioni salvate si riprendono solo con indirizzi HTTPS.
- **Evidenza:** test del client su origini, colore dell'avatar, pagine e gruppi; Clippy Linux e Windows. **Sospeso:** prova visiva con un Core reale e compilazione dello **SwiftUI** (solo CI su macOS); accesso utente nella console SwiftUI non previsto.

### ADR-0169 — Utenti con login, registrazione e personalità

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (Core e API; interfacce in ADR-0170).
- **Decisione:** il Core resta a un solo proprietario, ma può avere più **utenti** (`docs/USERS.md`). Account in tabelle dedicate dello stesso database (`users`, `user_sessions`, `user_invites`, `user_messages`, `user_memories`): nessuna query esistente li tocca, quindi **non c'è nulla da filtrare per sbaglio**. Registrazione **solo su invito monouso** (predefinito; `closed` o `open` a scelta del proprietario); password con Argon2id (19 MiB, 2 passaggi, salt casuale) e regole minime; sessioni con token casuale a 256 bit di cui si conserva l'hash (30 giorni, revocabili, chiuse alla disattivazione dell'account e al cambio password); intestazione dedicata `x-jarvis-user` valida **solo** su `/api/auth/*` e `/api/me*`, senza concedere nulla dell'area del proprietario né dei dispositivi (la lista dei percorsi dei dispositivi non cambia). Difese: blocco dopo 5 errori per nome utente (15 minuti) e limite globale, risposte e tempi uguali per nome inesistente, password errata e account disattivato, **al massimo 2 calcoli di Argon2 insieme** e controllo dell'invito prima dell'hash, 2 chat cloud insieme riservate agli utenti (gli slot del proprietario restano suoi), 30 messaggi ogni 10 minuti per utente, 50 utenti, 400 messaggi e 100 ricordi ciascuno. **Nessuno strumento**: la chat dell'utente è una richiesta al modello con un prompt dedicato (`prompts::user_system`: stesso nucleo e ruoli, personalità e profilo di quell'utente, i suoi ricordi, regole di account senza strumenti) e la sua cronologia; usa il modello cloud del proprietario solo se questi attiva `cloud` (spento di default: costo e riservatezza). **Personalità per utente** in `users/<id>/persona.json` e `profile.json` con gli stessi validatori del proprietario; `persona::update_for` non attiva mai la personalità globale, quindi quella del proprietario non cambia; i messaggi dell'area utenti sono neutri (non usano la sua forma di rivolgersi). La personalità nasce alla registrazione (nome, tono informale, questionario facoltativo), si rifinisce a mano e si impara dalla chat con comandi espliciti («chiamami…», «dammi del tu/lei», «sii più diretto/cordiale/formale», «ricorda che…», «dimentica tutto»). Messaggi e ricordi sono cifrati con la chiave della chat; il proprietario vede solo conteggi, mai il testo; audit per registrazione, accesso (riuscito e fallito), uscita, personalità, password, eliminazione e per le azioni del proprietario (`users.*`), senza segreti.
- **Perché così:** isolare un Core pensato per un solo proprietario con filtri su ogni funzione (memoria, note, piani, strumenti) avrebbe lasciato troppe strade per leggere i dati di altri; gli utenti hanno invece una superficie piccola, esplicita e senza strumenti.
- **Non incluso:** strumenti per gli utenti, streaming, recupero della password via email, accesso con provider esterni, protezione dall'amministratore del sistema (ha accesso alla chiave di cifratura).
- **Evidenza:** 7 test d'integrazione con l'API vera (invito monouso e politica, nessuna enumerazione, blocco dopo 5 errori, sessioni e token del proprietario non intercambiabili, isolamento di ricordi e cronologia, personalità che non tocca quella del proprietario, consenso al cloud, gestione del proprietario senza testi, audit senza segreti), test unitari di password, hash, impostazioni e comandi, e del payload con cronologia. **Sospeso:** prova con un modello cloud reale (nessuna chiave qui).

### ADR-0168 — Barra laterale, avatar e resa delle risposte

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** **barra laterale** più grande (voci da 44 punti con testo da 15,5, riquadro dell'icona da 30; su macOS testo da 15 e margini più ampi), **ridimensionabile** (da 232 a 360 punti, la larghezza resta fra un avvio e l'altro) con maniglia sul bordo; **scorrimento** con barra piena e visibile, sfumature in alto e in basso quando c'è altro da vedere e voce attiva portata in vista (con animazione) quando la pagina cambia da un'altra parte dell'app. **Avatar**: stato di *allerta* (arresto di emergenza o errore: colori caldi e anello che pulsa piano), *onda verde* a ogni operazione riuscita e **colore d'accento** per personalità (egui; su macOS tonalità regolabile con la stessa interfaccia); «Riduci animazioni» lascia il disegno fermo. **Risposte**: i messaggi di chat in egui ora mostrano Markdown leggero (blocchi di codice con «Copia», grassetto, codice in linea, elenchi, titoli; blocco non chiuso trattato come codice mentre la risposta arriva, come già su macOS) e le istruzioni ai modelli chiedono lunghezza calibrata sulla domanda, **grassetto** per il dato chiave nelle risposte lunghe e comandi, file e funzioni fra apici inversi.
- **Evidenza:** test del renderer (blocchi, grassetto, codice in linea, elenchi, titoli, segni isolati) e del colore dell'avatar; Clippy Linux e Windows e test del client; test del prompt. **Sospeso:** resa visiva e animazioni (nessuno schermo qui) e compilazione dello Swift (solo CI).

### ADR-0167 — Pagine «Agenti» e «Pacchetti» nelle app macOS e Windows

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (fase L, parte).
- **Decisione:** due pagine del solo proprietario, senza logica nel client: **Agenti** (da `/api/agents/supervisor`: stato, lavori dell'ultima ora, prestazioni, ruolo; azioni «Metti in pausa»/«Riattiva» e «Imposta il limite orario» da 1 a 500, vuoto per toglierlo, tramite `POST /api/agents/limits`) e **Pacchetti** (da `/api/plugins`: versione, autore, competenze e procedure portate, impronta; «Rimuovi» con `POST /api/plugins/remove`; area di testo per incollare un pacchetto JSON e «Installa» con `POST /api/plugins`). Il client controlla solo che il testo sia un oggetto JSON e che il limite sia nell'intervallo; ogni altra validazione, tutto-o-niente compreso, resta nel Core. La risposta del supervisore ora include `name`, `role` e `performance`. I dispositivi associati non vedono queste pagine (i loro percorsi non sono tra quelli consentiti). Android non cambia.
- **Evidenza:** test del Core sui nuovi campi; Clippy e test del client egui (anche per il bersaglio Windows) con i test della navigazione; la parte SwiftUI è compilata solo dalla CI.

### ADR-0166 — Hugging Face come server MCP e client MCP con sessioni

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** Hugging Face si collega **come server MCP autorizzato dal proprietario** (`https://huggingface.co/mcp`, token di sola lettura facoltativo nel deposito dei segreti), non come provider dentro il Core: dà accesso a ricerca di modelli e dataset senza nuovo codice per i provider e con le garanzie già in vigore (conferma a ogni `mcp.call`, risultati non attendibili, nessuna chiave restituita). Il sondaggio del server pubblico ha mostrato requisiti che il client non soddisfaceva, quindi **il client MCP ora segue Streamable HTTP**: `Accept` con `application/json` e `text/event-stream`, notifica `initialized`, identificativo di sessione (`Mcp-Session-Id`, validato) riportato nelle richieste, lettura di risposte SSE già concluse. Provider cloud, embeddings e voce su servizi esterni **restano fuori** finché non c'è una regola di riservatezza per i dati personali.
- **Evidenza:** il server fittizio dei test rifiuta le richieste senza `Accept` completo o senza sessione (come quello reale); test unitari del filtro dell'identificativo di sessione e delle risposte SSE; sequenza `initialize`, notifica e `tools/list` verificata con `curl` sul server reale (HTTP 202 per la notifica, strumenti elencati). **Sospeso:** prova end-to-end del client JARVIS contro il server reale (qui la rete passa da un proxy che il client non usa) e chiamate con token.

### ADR-0165 — Pacchetti di estensione dichiarativi (SDK senza codice di terzi)

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (fase J della roadmap, nella forma sicura).
- **Decisione:** un pacchetto `jarvis.plugin/1` è un JSON con competenze e modelli di piano, installabile dal solo proprietario (`GET/POST /api/plugins`, `POST /api/plugins/remove`). Non contiene codice, non usa la rete e non porta chiavi; campi non previsti sono rifiutati e il peso massimo è 64 KiB. L'installazione è **tutto o niente** (validazione completa prima di scrivere; aggiornamento non valido = versione precedente intatta), i componenti passano dagli stessi validatori di quelli del proprietario (strumenti del catalogo, anche in `undo`, passi, dipendenze, condizioni) e restano soggetti a policy, conferme, budget e audit: un pacchetto non può dare strumenti, togliere conferme o allargare permessi. I modelli prendono il nome `<pacchetto>-<modello>`; la rimozione elimina esattamente ciò che il pacchetto aveva portato (registro `plugins.json`). Audit `plugin.install` (con impronta SHA-256 nella risorsa) e `plugin.remove`.
- **Perché senza codice:** caricare codice nativo di terzi nel Core, che gestisce permessi, segreti e audit, sarebbe la via più diretta per aggirarli. Prima servono esecuzione isolata e firma degli autori.
- **Non incluso:** strumenti nativi da cartella, firma e distribuzione dei pacchetti, interfaccia grafica (oggi via API).
- **Evidenza:** test unitari della validazione (strumenti sconosciuti, campi non ammessi, nomi, limiti, duplicati, pacchetto vuoto) e test d'integrazione: installa, usa la procedura (piano riuscito), aggiorna senza duplicare, versione non valida rifiutata, rimuove tutto.

### ADR-0163 — Auto-miglioramento controllato: JARVIS propone, il proprietario approva

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (fase K della roadmap, nella forma sicura).
- **Decisione:** `GET /api/improvements` analizza **solo evidenze locali** (piani riusciti, esiti degli strumenti, prestazioni degli agenti) con regole deterministiche, senza inviare nulla a un modello: (1) **procedura ricorrente**, cioè la stessa sequenza di almeno 2 strumenti riuscita 3 o più volte e non ancora salvata; (2) strumento con almeno 5 esecuzioni e metà o più errori; (3) agente con almeno 5 lavori e meno del 50% di successi. L'unica azione applicabile è `POST /api/improvements/apply` per la procedura: salva il piano più recente come modello personale (stesse regole di nome e limiti di ADR-0159). Gli altri sono suggerimenti da valutare a mano (`apply` risponde 400). `POST /api/improvements/dismiss` nasconde un suggerimento (ultimi 200 ricordati). Entrambe le azioni sono del solo proprietario e finiscono nell'audit (`improvement.apply`, `improvement.dismiss`).
- **Cosa non fa, per scelta:** non modifica codice, regole di sicurezza, permessi, chiavi, prompt o catalogo degli strumenti; non scrive memorie né competenze da sola; non esegue le procedure salvate. Un sistema che si riscrive da solo renderebbe impossibile verificare i confini di sicurezza.
- **Evidenza:** test unitari delle regole (soglie, modelli già salvati, nomi in conflitto, piani con un solo passo) e test d'integrazione con piani reali: tre piani uguali → suggerimento → applica → modello salvato e suggerimento sparito; scartato non ritorna; strumento inaffidabile segnalato senza azione.

### ADR-0164 — Supervisione degli agenti: pausa e quota oraria decise dal proprietario

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (fase I della roadmap).
- **Decisione:** `GET /api/agents/supervisor` mostra per ogni specializzazione regole, lavori dell'ultima ora e stato (attivo, in pausa, quota esaurita); `POST /api/agents/limits` (`agent`, `enabled`, `max_runs_per_hour` da 1 a 500 o `null`) le modifica, solo dal proprietario e con audit `agent.limits`. L'instradamento (`route_agent`) applica la supervisione: un agente in pausa o oltre quota **non lavora** e la richiesta va al generalista, che resta sotto le stesse policy, conferme, budget e audit. La supervisione **restringe e non allarga**: non dà strumenti, permessi né autonomia in più. `/api/agents` mostra la supervisione accanto alle prestazioni e `/api/agents/route` spiega l'effetto sulla richiesta. **Conflitti:** se una richiesta tocca più specializzazioni decide il generalista; la sicurezza difensiva (agente `monitoring`) ha la precedenza e non è soggetta alle pause per non togliere le restrizioni difensive.
- **Non incluso:** priorità fra agenti in parallelo, quote di tempo o di memoria per agente, interfaccia grafica dedicata (oggi via API).
- **Evidenza:** test unitari delle decisioni e della validazione delle regole; test d'integrazione: agente messo in pausa → il generalista lavora; riattivato con limite → torna attivo; agente sconosciuto rifiutato.

### ADR-0162 — Strumenti per documenti: PDF, fogli XLSX e appunti

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (fase E della roadmap).
- **Decisione:** `pdf.extract` (testo di un PDF della workspace), `sheet.read` (righe e nomi dei fogli di un XLSX, con `summary` il riepilogo delle colonne come `table.summary`), `clipboard.read` e `clipboard.write` (appunti di sistema). Nessuna dipendenza nuova: ZIP (voci memorizzate o deflate, con limite di espansione contro le «bombe»), XML minimo e flussi di contenuto PDF sono letti con `flate2`, già nel Core. I file si leggono solo dalla workspace (30 MiB al massimo). `pdf.extract` e `sheet.read` sono di sola lettura, a rischio basso, eseguibili in parallelo nei piani e restituiscono il contenuto marcato `untrusted`: è un dato, mai un'istruzione. Gli appunti possono contenere password e testi ostili: **la conferma è sempre richiesta** sia in lettura sia in scrittura (rischio medio, nessun nuovo tentativo automatico) e il testo letto è marcato `untrusted`; senza sessione grafica e programma di sistema (pbcopy/pbpaste, PowerShell, wl-clipboard, xclip, xsel) l'errore è `clipboard.unavailable`.
- **Limiti dichiarati:** PDF solo con testo semplice (codifica Standard/WinAnsi o UTF-16): niente OCR, niente file cifrati (`pdf.encrypted`), niente font con codifica CID/ToUnicode (`pdf.no_text` con la spiegazione), impaginazione approssimata; XLSX senza ZIP64, date come numeri seriali, formule come ultimo valore calcolato. Non sono incluse la scrittura di PDF e XLSX.
- **Contratto:** con i nuovi strumenti il catalogo supera 96 voci; il limite di `delegation.schema.json` per gli strumenti proposti a un agente passa da 96 a 128 (aggiunta compatibile come in ADR-0046 e 0121), allineato a `MAX_DELEGATED_TOOLS`.
- **Evidenza:** test su archivi ZIP, fogli e PDF sintetici (anche compressi, cifrati e senza testo), test del piano con file veri nella workspace e test che gli appunti restano in attesa di conferma.

### ADR-0161 — Ripianificazione: un piano di recupero proposto dal modello, sempre come bozza

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (completa la fase B).
- **Decisione:** `POST /api/plans/{id}/replan` (`create` facoltativo) chiede al modello un piano di recupero per un piano **fallito, annullato o interrotto** (o con passi falliti). Al modello va un rapporto con obiettivo e, per ogni passo, id, strumento, stato e codice/messaggio di errore ripuliti e accorciati; **non** vanno argomenti né risultati dei passi, che possono contenere contenuti privati, e un rapporto con credenziali non parte. Le istruzioni chiedono di non ripetere i passi riusciti, di cambiare approccio e di **non aggirare** permessi negati, conferme rifiutate o l'arresto di emergenza (in quel caso risponde senza passi e il Core mostra il motivo). La risposta passa dalla stessa validazione del pianificatore (strumenti del catalogo, cicli, riferimenti, campi riservati). Il recupero è una **nuova bozza** `pending` con `replan_of` e `replan_depth`; il piano originale non cambia. La catena è limitata a 3 ripianificazioni, poi decide il proprietario. Con l'arresto di emergenza attivo né proposta né ripianificazione partono. Audit `plan.replan`.
- **Perché:** il recupero automatico che esegue da solo amplierebbe l'autonomia oltre ciò che il proprietario ha approvato; qui il modello suggerisce e il proprietario sceglie.
- **Non incluso:** esecuzione automatica del recupero, riuso dei risultati dei passi riusciti nel nuovo piano.
- **Evidenza:** test unitari del rapporto (nessun argomento o risultato), test d'integrazione con piani realmente falliti (bozza collegata, originale invariato, catena limitata, arresto di emergenza) e controlli e2e.

### ADR-0160 — Pianificatore guidato dal modello: il modello propone, il Core valida, il proprietario decide

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (fase B della roadmap).
- **Decisione:** `POST /api/plans/propose` (`goal`, `create` facoltativo) chiede al modello cloud configurato un piano a grafo in JSON. Il prompt elenca solo gli strumenti del catalogo con riepilogo, argomenti e rischio, esclude `mission.*` e `plan.*` e dichiara che obiettivo e risultati sono dati, non istruzioni. Il Core **non si fida della risposta**: estrae il JSON (anche da recinti di codice), rimuove i campi riservati (`id`, `status`, `idempotency_key`, `budget`…), applica le stesse regole di un piano scritto a mano (passi, dipendenze, cicli, riferimenti, condizioni), scarta gli strumenti sconosciuti o interni anche in `undo`, e aggiunge `max_risk` e il rischio di ogni passo. L'esito è una **bozza**: con `create` diventa un piano `pending` salvato, mai avviato; l'esecuzione resta una chiamata esplicita, passo per passo sotto policy, regole del proprietario, conferme, budget e audit. L'obiettivo con credenziali non lascia il Core (`contains_credentials`); senza chiave cloud la risposta è 503 «unavailable». Azione registrata nell'audit (`plan.propose`).
- **Perché:** il modello decide cosa fare, ma i confini (strumenti, permessi, conferme) restano deterministici nel Core.
- **Non incluso:** ripianificazione automatica dopo un fallimento, modello locale, verifica indipendente del piano con un secondo modello.
- **Evidenza:** test unitari del parsing e della validazione, test d'integrazione con risposte simulate del modello (bozza non avviata, strumenti sconosciuti, risposte non valide, nessun piano creato su errore) e controlli e2e.

### ADR-0154 — Esecuzione parallela dei piani e condizioni di successo

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario di completare le parti «Pianificate» (fasi A e C della roadmap).
- **Decisione:** i passi pronti, indipendenti e **senza stato** (calcoli, lettura di file nella workspace, Git in sola lettura, analisi, ricerca e lettura Web: `toolsys::parallel_safe`) girano insieme, fino a 4, ciascuno in un thread con un archivio privato e vuoto. Policy e audit restano **sequenziali e prima** dell'esecuzione; esiti, cronologia di salute e audit sono registrati dopo, in ordine. Gli strumenti con stato (note, memoria, sandbox, workflow, shell, scritture) e quelli che richiedono conferma non entrano mai in un gruppo parallelo. Ogni passo può dichiarare `expect`: condizioni (`exists`, `not_empty`, `eq`, `ne`, `gt`, `ge`, `lt`, `le`, `contains`) sul proprio risultato; se non sono soddisfatte il passo fallisce con `plan.expectation_failed` e vale come errore (ripetizioni e `on_error` compresi). Nuovo strumento `time.wait` (0–10 s) per distanziare i passi.
- **Evidenza:** tre attese da un secondo completate in meno di 2,4 s; test delle condizioni; tutti i test dei piani precedenti invariati.

### ADR-0155 — Strumenti di sviluppo e dati: Git, log, dipendenze, tabelle

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (fase D).
- **Decisione:** `git.status`, `git.log`, `git.diff`, `git.branch` (sola lettura) e `git.commit` (**conferma richiesta**, `--no-verify`); Git gira senza shell, con argomenti fissi, timeout di 10 s, output limitato e hook, `fsmonitor`, programmi di confronto esterni e protocolli pericolosi disattivati; percorsi solo dentro la workspace. `log.analyze` riassume un log (livelli, problemi ricorrenti normalizzati, intervallo, ultimi errori); `deps.list` legge `Cargo.toml`, `package.json`, `requirements.txt` e `go.mod` e segnala le dipendenze non fissate; `table.summary` riassume un CSV (tipi, statistiche, valori frequenti, anteprima). Nessuno usa la rete.
- **Evidenza:** test su repository Git reale (stato, commit, differenze, cronologia, hook non eseguiti), log, manifest e CSV sintetici.

### ADR-0156 — Client MCP con server autorizzati dal proprietario

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (fase F).
- **Decisione:** `mcp.list` (elenca gli strumenti, rischio medio) e `mcp.call` (chiama uno strumento, **rischio alto e conferma sempre richiesta**). I server si autorizzano solo dal computer del proprietario (`/api/mcp/servers`: nome, indirizzo, token facoltativo nel deposito dei segreti, mai restituito); né un modello né un piano possono aggiungerne. Solo HTTPS (HTTP su loopback), niente credenziali nell'indirizzo, niente reindirizzamenti, risposta massima 256 KiB, timeout 20 s, fino a 8 server. Il risultato è marcato `untrusted` e passa dalla guardia anti-iniezione: è un dato, mai un'istruzione.
- **Non incluso:** flussi SSE a lunga durata e server che richiedono OAuth (le sessioni e le risposte SSE concluse sono in ADR-0166).
- **Evidenza:** test con un server MCP locale: token inviato, elenco, chiamata con conferma, testo ostile riconosciuto, server sconosciuto rifiutato.

### ADR-0157 — Digital Environment Twin e valutazione degli agenti

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout (fasi H e I, parte).
- **Decisione:** `GET /api/twin` riunisce, dalle fonti esistenti, Core (versione, sistema, arresto di emergenza, cifratura), dispositivi e nodi (con capacità e stato), servizi (Docker, provider, server MCP, sandbox), connettori, progetti (missioni, piani, automazioni, cose da fare, chat, ricordi) e strumenti disponibili su questo sistema per categoria. Non contiene segreti, percorsi né contenuti di chat, memoria o file. Ogni lavoro delegato a una specializzazione registra esito, durata e strumenti in `agent_runs`; `GET /api/agents` mostra per ciascun agente lavori, tasso di successo e medie.
- **Non incluso:** supervisione attiva, risoluzione dei conflitti fra agenti e limiti di risorse per agente oltre ai passi massimi.
- **Evidenza:** test API del gemello e delle prestazioni degli agenti.

### ADR-0158 — Automazioni guidate da eventi: osservazione di file

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** regole `/api/watches` (percorso nella workspace + workflow per id o nome) gestite dal solo proprietario, con al massimo 16 regole. A ogni giro dello scheduler si confronta la firma (dimensione e data; per le cartelle numero di voci e voce più recente); la prima lettura è il punto di partenza e non avvia nulla; un cambiamento avvia il workflow una sola volta con un'esecuzione di tipo `event` (nuovo valore del contratto `workflow-run`). Il workflow conserva le proprie approvazioni: la regola non le amplia. Con l'arresto di emergenza il controllo non avviene.
- **Evidenza:** test del confronto delle firme e test d'integrazione con un workflow reale (una sola esecuzione, contratto valido, arresto di emergenza).

### ADR-0159 — Modelli di piano: procedure ricorrenti approvate dal proprietario

- **Data:** 2026-11-25 · **Stato:** implementata nel checkout.
- **Decisione:** modelli con parametri (`{{nome}}`, tipi conservati per i segnaposto interi, predefiniti e obbligatori): cinque incorporati in sola lettura (diagnosi del sistema, stato del repository, analisi di un log, riepilogo di una tabella, controllo di sicurezza) e fino a 30 personali. **L'apprendimento di procedure è controllato**: nulla viene salvato da solo; il proprietario salva un piano riuscito come procedura (`POST /api/plans/templates`, con nome valido e mai quello di un modello incorporato) e poi la ripete con `POST /api/plans/from-template`, sempre entro policy, conferme e budget.
- **Evidenza:** test della sostituzione dei parametri, dei tipi, dei parametri mancanti e dell'intero ciclo salva, esegui, elimina.

### ADR-0135 — Correttore dei messaggi scritti

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora il correttore grammaticale di prompt»).
- **Decisione:** `promptfix.rs` corregge in locale e senza modelli ciò che è inequivocabile: accenti mancanti («perche'» → «perché», «e'» → «è», decine di parole in -tà), abbreviazioni da chat («nn», «cmq», «xche»), spazi e punteggiatura («ciao,come» → «ciao, come», «va !» → «va!»), elisioni staccate, parole di funzione doppie, «un po» → «un po'», maiuscole a inizio frase, JARVIS. Non tocca mai codice (blocchi e `inline`), percorsi, URL, indirizzi, numeri, nomi con maiuscole interne o parole sconosciute; il risultato è stabile (correggere due volte non cambia nulla).
- **Uso:** la chat applica il correttore **senza cambiare le maiuscole** (un comando o un ricordo non devono cambiare iniziale) e restituisce `corrected {text, changes}` solo se è cambiato qualcosa; `POST /api/prompt/check` mostra la versione completa con maiuscole; lo strumento `text.correct` (anche via MCP e offline: «correggi: …») serve alla competenza di scrittura. `JARVIS_PROMPT_FIX=off` lo disattiva.
- **Limiti:** non sostituisce una revisione grammaticale con un modello: non cambia lessico né sintassi.

### ADR-0136 — Prompt di sistema specializzati per ruolo e modello

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora il system prompt di ogni modello e sottomodello e specializzalo in più modelli… risposte più coerenti»).
- **Decisione:** `prompts.rs` compone il prompt a livelli senza ripetizioni: nucleo comune (ora con la regola di **coerenza**: stessi nomi, cifre, terminologia e decisioni, nessun cambio di registro, correzioni dichiarate), data e personalità, **profilo** del proprietario, guida del percorso (veloce/ragionamento/codice/contesto lungo), **ruolo** della richiesta e **stile per famiglia di modello**. Dieci ruoli riconosciuti con regole deterministiche (direzione aziendale, sicurezza, IT, sviluppo, ricerca, traduzione, scrittura, analisi dati, pianificazione, didattica); stili distinti per modelli veloci, di ragionamento, a contesto lungo e per il codice, scelti dal nome del modello e dal percorso.
- **Limiti:** nessun livello amplia permessi, policy o conferme; il prompt resta sotto i 6500 caratteri (test).
- **Non verificato:** l'effetto sulla qualità con provider reali.

### ADR-0137 — Collegamento dei dispositivi senza toccare host o file

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora il collegamento tra più dispositivi senza modificare host… in maniera più facile»).
- **Problema:** per associare un telefono servivano un file `remote-hosts.txt` o una variabile d'ambiente con riavvio, poi indirizzo e codice da digitare.
- **Decisione:** (1) **indirizzi pubblici dall'app**: `GET/PUT/POST /api/devices/access` (`add`/`remove`) salva `remote-hosts.txt` e aggiorna subito l'elenco degli host ammessi (nessun riavvio, nessuna variabile); accetta anche un indirizzo copiato per intero. (2) **collegamento di associazione**: ogni invito include `link` (`jarvis://pair?origin=…&code=…`); su macOS si condivide o si copia, su Windows si copia. (3) **Android** apre il collegamento da un messaggio o dal browser (intent `jarvis://pair`), oppure lo legge dagli appunti con «Incolla il collegamento»: indirizzo e codice si compilano da soli (`PairingLink`, solo `https`, nomi completi, niente credenziali nell'URL).
- **Sicurezza invariata:** il codice resta monouso e a scadenza, il dispositivo riceve il proprio token, l'accesso remoto passa solo da host HTTPS dichiarati dal proprietario.
- **Non incluso:** scoperta automatica sulla rete locale (richiederebbe un listener sulla LAN con certificato fissato: decisione di sicurezza da prendere a parte) e codice QR.

### ADR-0138 — Server MCP

- **Data:** 2026-11-24 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario («migliora gli MCP e API»).
- **Decisione:** `POST /api/mcp` espone gli strumenti di JARVIS ad altri agenti con il Model Context Protocol (JSON-RPC 2.0 su HTTP, risposte JSON, richieste singole o in lotto fino a 32): `initialize`, `ping`, `tools/list`, `tools/call`; le notifiche ricevono 202. Autenticazione: lo stesso token del proprietario (`X-Jarvis-Token` o `Authorization: Bearer`); i dispositivi associati non hanno accesso. `JARVIS_MCP=off` lo disattiva.
- **Cosa espone:** solo calcolo e sola lettura senza dati personali: i sette calcoli aziendali, IVA e finanziamenti, `time.now`, `date.calc`, `math.calc`, `units.convert`, `text.count`, `text.correct`, `code.validate_json`, `security.cvss`, `it.subnet`, `system.info`, `system.disks`. Restano fuori file, memoria, note, schermo, applicazioni, terminale, rete, connettori e nodi.
- **Controlli:** ogni chiamata crea un task, passa dalla policy e dai permessi del proprietario ed è registrata nell'audit come l'agente interno; argomenti validati dallo schema dello strumento. I nomi usano `__` al posto del punto.
- **API:** inoltre `POST /api/prompt/check`, `GET/PATCH /api/profile`, `POST /api/voice/wake`, `GET/PUT /api/devices/access`, `POST /api/cloud/chat/stream`; l'invito ora porta `link`.

### ADR-0139 — Avatar in ascolto e grafica

- **Data:** 2026-11-24 · **Stato:** implementata su macOS · **Fonte:** richiesta del proprietario («migliora ulteriormente l'avatar… e la grafica»).
- **Decisione:** nuovo stato «in ascolto» dell'avatar macOS (si fonde con elaborazione e parlato): anelli che convergono verso il volto, spessore legato al livello del microfono, occhi più aperti e alone più vivo; la barra laterale mostra lo stato dell'ascolto con icona del microfono che pulsa e onda sonora attiva durante la registrazione. Pagina «Profilo» con avanzamento, campi e preferenze; pagina Dispositivi con indirizzo pubblico e condivisione del collegamento.
- **Non verificato:** resa a schermo (Swift solo in CI).

### Decisione operativa — Merge automatico con CI verde

- **Data:** 2026-11-17 · **Fonte:** decisione di manutenzione del progetto.
- **Regola:** dettagliata in [DEVELOPMENT.md](DEVELOPMENT.md#integrazione-delle-modifiche).

### ADR-0085 — Nuovo marchio applicativo generato da vettori

- **Data:** 2026-11-22 · **Stato:** implementato nel checkout · **Fonte:** richiesta del proprietario di rifare l'icona da zero.
- **Decisione:** sostituire il badge dettagliato con nome incorporato con un monogramma J luminoso, essenziale e riconoscibile alle dimensioni ridotte. `scripts/make_brand_assets.py` disegna il marchio proceduralmente e genera asset per macOS, Windows, Android e installer senza dipendere da immagini esterne; la precedente sorgente raster non è più usata.
- **Conseguenze:** il nome JARVIS resta nel contesto dell'app/installer e non deve essere letto nell'icona stessa. Colore scuro e ciano restano coerenti con l'interfaccia; tutte le piattaforme riusano il medesimo segno.
- **Evidenza:** rigenerazione degli asset in un'unica procedura; convalida dimensioni, alpha e ripetibilità prima della CI e del controllo visivo su dispositivi reali.
- **Documenti interessati:** `scripts/make_brand_assets.py`, risorse desktop e Android, `INTERFACE_AND_DEVICES.md`, ROADMAP.md.

### ADR-0086 — Voce JARVIS originale e risposte ancorate alle evidenze

- **Data:** 2026-11-22 · **Stato:** implementata nel checkout · **Fonte:** richiesta del proprietario di migliorare personalità e accuratezza dell'assistente.
- **Decisione:** adottare un tono calmo, competente, cordiale e conciso, con ironia discreta non imitativa. Il prompt richiede di distinguere fatti, inferenze e stime, rendere esplicita l'incertezza, verificare gli esiti degli strumenti e ammettere i limiti. Per richieste composte, gli strumenti si usano in sequenza, un passo per volta.
- **Limiti:** l'aggiornamento istruisce i provider configurati; non effettua fine-tuning o addestramento dei pesi, non garantisce di soddisfare richieste prive di strumenti o autorizzazioni e non amplia policy, permessi o conferme.
- **Evidenza:** test unitario del prompt di sistema; la qualità resta da misurare su una suite di valutazione sintetica e su provider reali, senza credenziali nel checkout.
- **Documenti interessati:** `core/server/src/cloud.rs`, `AI_SYSTEM.md`, ROADMAP.md.

### ADR-0087 — Collegamenti espliciti e dati affidabili fra client e Core

- **Data:** 2026-11-22 · **Stato:** implementata su `main` (`1c3679f`) · **Fonte:** richiesta di migliorare i collegamenti desktop–Android, multi-Core e connettori esterni.
- **Contesto:** Android riceve notifiche con destinazioni valide per il contratto ma non supportate dal client; i riepiloghi di Core offline o le richieste attività fallite presentavano risultati sconosciuti come zeri; l'avvio OAuth non segnalava risposte senza URL di autorizzazione.
- **Decisione:**
  - Android mantiene la navigazione diretta per Chat, Attività e Plancia. Per altre viste valide ma non supportate mostra un'indicazione esplicita, senza instradare verso una schermata diversa né ampliare i permessi.
  - I contatori di Core non raggiungibili e il numero di attività recenti quando la richiesta fallisce sono trattati come sconosciuti e mostrati come «—». I dati disponibili dagli altri Core continuano a essere aggregati in sola lettura.
  - Il desktop apre per OAuth solo URL HTTPS senza credenziali incorporate. Una risposta priva di URL sicuro diventa un errore visibile; dopo l'avvio viene indicato di completare l'autorizzazione e aggiornare lo stato. L'utente deve comunque premere Autorizza.
- **Motivazione:** rendere esplicito quando un collegamento o un dato non è disponibile, senza inventare esiti e senza spostare chat, mutazioni o credenziali fra Core.
- **Conseguenze:** le viste non supportate restano tali su Android; non si aggiungono scope, inoltro delle operazioni o autorizzazioni implicite. La prova della resa Android e del flusso OAuth reale resta dipendente da SDK/dispositivi e provider configurati.
- **Evidenza:** i test desktop coprono URL OAuth e contatori offline; `cargo check`, Clippy e compilazione/link dei test per Windows superati. Il run CI `45067836302` sulla revisione `1c3679f` è verde, inclusi desktop macOS e workflow Android/APK release. La conferma OAuth con un account reale autorizzato resta un collaudo operativo.
- **Documenti interessati:** `README.md`, `INTERFACE_AND_DEVICES.md`, ROADMAP.md, app desktop e Android.

### ADR-0088 — Output limitato e guard-rail del terminale

- **Data:** 2026-11-22 · **Stato:** implementata nel checkout.
- **Contesto:** `shell.run` drenava stdout e stderr interamente prima di troncarli, consentendo output voluminosi in memoria. Il controllo dei comandi elevati non riconosceva in modo affidabile separatori whitespace, percorsi e nomi eseguibili Windows.
- **Decisione:** conservare al massimo 8.000 caratteri per stream, drenando comunque i byte restanti; decodificare senza tagliare sequenze UTF-8 e restituire esplicitamente gli errori di lettura. Negare i token `sudo`, `doas`, `pkexec` e `runas` dopo la normalizzazione di separatori e percorsi, oltre ai comandi distruttivi già bloccati.
- **Motivazione:** porre un limite reale al buffering e ridurre varianti banali di comandi elevati senza presentare il filtro come un confine di sicurezza.
- **Conseguenze:** il terminale continua a eseguire comandi con i permessi dell'utente dopo consenso e non è una sandbox; il timeout e la policy esistenti restano invariati. Il filtro non sostituisce l'isolamento di processo.
- **Evidenza:** test per output ampio, UTF-8, troncamento e varianti di whitespace/percorso; intera suite Core superata (76 test, 1 ignorato perché richiede Docker), fmt, Clippy con `-D warnings` e `git diff --check` superati.
- **Documenti interessati:** `SECURITY_AND_PRIVACY.md`, `AUTONOMY_AND_TOOLS.md`, `core/server/src/tools.rs`.

### ADR-0089 — Competenze affidabili e fallimenti espliciti

- **Data:** 2026-11-22 · **Stato:** implementata nel checkout.
- **Contesto:** il briefing predefinito poteva suggerire dati email/calendario senza connettori disponibili; errori nel caricamento delle competenze potevano apparire come una lista vuota e i guasti nel calcolo dell'ambito degli strumenti non erano esplicitamente registrati.
- **Decisione:** aggiornare il briefing predefinito affinché dichiari quando le fonti non sono disponibili, senza sovrascrivere personalizzazioni dell'utente; aggiungere le competenze incluse **Sicurezza difensiva** e **Sviluppo e revisione del codice** senza nuovi permessi; propagare gli errori di storage delle competenze e registrare i guasti di calcolo dell'ambito, omettendo gli strumenti suggeriti in caso di errore.
- **Motivazione:** rendere i limiti e i guasti osservabili, mantenendo procedure di sicurezza circoscritte e senza attribuire alle competenze poteri aggiuntivi.
- **Conseguenze:** policy, conferme e audit restano invariati. La migrazione modifica la descrizione del briefing soltanto se corrisponde ancora al contenuto incluso originariamente.
- **Evidenza:** test per errori storage, migrazione non distruttiva e attivazione delle competenze; suite Core completa superata (76 test, 1 ignorato perché richiede Docker), fmt, Clippy con `-D warnings` e `git diff --check` superati. Test contratti (14) e AI (31; uno saltato per `openwakeword` assente) superati.
- **Documenti interessati:** `AUTONOMY_AND_TOOLS.md`, `SECURITY_AND_PRIVACY.md`, `core/server/src/skills.rs`, `core/server/src/orchestrator.rs`.

### ADR-0090 — Routine n8n affidabili e container Docker locali isolati

- **Data:** 2026-11-22 · **Stato:** implementata nel checkout.
- **Contesto:** il registro delle routine n8n trattava errori di lettura o JSON malformato come lista vuota; anche errori nel rilevamento delle esecuzioni riuscite e nella registrazione delle notifiche venivano nascosti. Il sandbox Python usava il contesto Docker predefinito, configurabile dall'ambiente/CLI, anziché fissare il daemon locale.
- **Decisione:**
  - Leggere e validare il registro delle routine prima di usarlo; trattare come vuoto solo il file non ancora creato e riportare errori o record non validi nello stato n8n. Propagare gli errori del controllo delle esecuzioni e registrare esplicitamente i fallimenti di notifica e acknowledgement.
  - Usare un helper Docker condiviso che seleziona il socket locale o la pipe Docker locale e rimuove le variabili che possono reindirizzare il client a un daemon remoto. Applicarlo sia a n8n sia alla sandbox Python.
  - Eseguire n8n come UID 1000 con root filesystem read-only, tmpfs limitati per `/tmp` e cache, `--init`, log driver locale con rotazione, rete disabilitata, capability drop, `no-new-privileges`, task runner disabilitati e limiti di risorse. Applicare le restrizioni filesystem anche al container amministrativo, che usa il volume persistente solo a servizio fermo.
  - Limitare il sorgente Python inline a 64 KiB e rendere espliciti gli errori di invio/lettura degli stream del container.
- **Motivazione:** preservare gli archivi in caso di guasto, rendere visibili esecuzioni perse e ridurre privilegi e scritture del container senza ampliare la rete, gli strumenti n8n o le capacità delle automazioni.
- **Conseguenze:** le routine n8n restano solo promemoria e verifiche audit predefinite, non workflow arbitrari. L'immagine fissata è `docker.n8n.io/n8nio/n8n:2.48.7`; la cache n8n deve restare temporanea ma scrivibile dall'UID 1000. Il daemon Docker locale resta una superficie privilegiata dell'host.
- **Evidenza:** suite Core (`cargo test --manifest-path core/Cargo.toml --locked -p jarvis-core`: 84 test unitari superati, tutte le suite d'integrazione superate; due test Docker normalmente ignorati sono stati eseguiti separatamente e superati), 14 test contratti, fmt, Clippy con `-D warnings` e `git diff --check`; integrazione n8n con avvio, importazione e rimozione della routine; smoke test Python Docker con rete disabilitata e senza workspace; test per sorgente oltre 64 KiB, archivio n8n corrotto/non valido ed endpoint Docker locale.
- **Documenti interessati:** `AUTONOMY_AND_TOOLS.md`, `SECURITY_AND_PRIVACY.md`, `core/server/src/container_runtime.rs`, `core/server/src/n8n.rs`, `core/server/src/sandbox.rs`, `core/server/src/api.rs`.

### ADR-0091 — Plancia desktop originale e layout adattivo

- **Data:** 2026-11-22 · **Stato:** implementata nel checkout; verifica visiva su macOS/Windows sospesa.
- **Contesto:** la Plancia mostrava azioni rapide in un pannello separato, aveva una gerarchia visiva piatta e la barra superiore poteva comprimersi male nelle finestre strette. L'immagine fornita dall'utente richiede un riferimento visivo più moderno, ma alcuni pannelli rappresentati non corrispondono a dati disponibili nel Core.
- **Decisione:** adottare una palette blu notte/ciano più contrastata, un riquadro hero a zone adattive con riepiloghi reali Core/AI, avatar vettoriale, alone animato a bassa intensità e azioni contestuali. Compattare la barra superiore nelle finestre strette; mantenere le schede statistiche semantiche e avvolgere gli elementi delle attività invece di farli uscire dalla card. Le attività recenti usano schede compatte con stato localizzato e colore semantico, metadati secondari e un'azione esplicita per preparare una bozza chat. Non mostrare agenti, progetti, risorse hardware o grafici senza API e dati attendibili; non aggiungere fotografie, risorse remote o dipendenze.
- **Motivazione:** riprendere la chiarezza della gerarchia del riferimento senza introdurre numeri fittizi, WebView o variazioni dei confini nativi macOS/Windows.
- **Conseguenze:** barra e navigazione restano fisse e lo scroll continua nel pannello centrale. «Riduci animazioni» ferma sia l'avatar sia l'alone decorativo. Le soglie di layout sono adattive; il collaudo grafico ai limiti di scroll su macOS/Windows resta necessario.
- **Evidenza:** `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check`, cross-check Windows `cargo check --target x86_64-pc-windows-gnu --locked --tests`, Clippy Windows con `--all-targets -D warnings`, compilazione/link dei test Windows con `cargo test --target x86_64-pc-windows-gnu --locked --no-run` e `git diff --check`, superati. I test unitari Windows non sono stati eseguiti su Linux. Il cross-check macOS è stato tentato ma il compilatore host GCC non supporta i flag Apple e non è installato un SDK Apple; l'host non offre una sessione grafica macOS/Windows. `tests/e2e/ui_smoke.cjs` non è presente nel checkout.
- **Documenti interessati:** `INTERFACE_AND_DEVICES.md`, ROADMAP.md e `apps/desktop/src-tauri/src/app.rs`.

### ADR-0092 — Avatar più ampio con animazione a livelli

- **Data:** 2026-11-23 · **Stato:** implementata nel checkout; verifica grafica nativa sospesa.
- **Contesto:** l'avatar vettoriale della Plancia risultava piccolo e il suo movimento, limitato a una pulsazione lieve, non lo rendeva abbastanza presente.
- **Decisione:** ampliare il ritratto desktop fino a 280 punti e quello Android a 248 dp. Aggiungere fluttuazione, scansione della visiera e orbita luminosa su entrambe le piattaforme, mantenendo il disegno locale senza immagini o dipendenze; «Riduci animazioni» continua a congelare il desktop e Android segue la scala animazioni di sistema.
- **Motivazione:** rendere l'avatar più riconoscibile e vivo, mantenendo la resa vettoriale, la compatibilità nativa e le preferenze di accessibilità.
- **Conseguenze:** la Plancia usa più altezza e lo scroll resta nel contenuto centrale; la schermata Android mantiene la barra inferiore fissa e scorre internamente.
- **Evidenza:** sul cambiamento, fmt Rust, `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-pc-windows-gnu --locked --tests`, Clippy Windows con `--all-targets -D warnings`, compilazione/link test Windows con `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-pc-windows-gnu --locked --no-run` e `git diff --check` superati. `gradle -p apps/android -Pjarvis.coreOnly=true :core-client:test --no-daemon` superato (task aggiornati). La compilazione Compose è stata tentata ma bloccata: Android SDK non configurato. La verifica grafica runtime su macOS/Windows/Android resta sospesa perché non sono disponibili i rispettivi sistemi e display.
- **Documenti interessati:** `INTERFACE_AND_DEVICES.md`, `ROADMAP.md`, `apps/desktop/src-tauri/src/app.rs` e `apps/android/app/src/main/java/local/jarvis/app/MainActivity.kt`.

### ADR-0093 — Importazione OAuth Google e modelli per le deleghe

- **Data:** 2026-11-23. **Stato:** implementata.
- **Decisione:** importare il JSON OAuth Google Desktop dal client locale tramite un endpoint dedicato a Gmail e Calendar. Gli endpoint del provider e gli scope provengono dal catalogo del Core. Un nuovo consenso sostituisce quello pendente dello stesso servizio; revoca e rimozione lo annullano.
- **Credenziali:** access token, refresh token e secret OAuth condividono una voce cifrata. Gli archivi precedenti contengono già gli stessi campi. Una futura revoca elimina anche l'eventuale voce access token storica.
- **Inferenza:** dare priorità al profilo di ragionamento per richieste con più passaggi. Un modello Ollama locale facoltativo risponde ai compiti testuali dei temporanei che il motore a regole non riconosce. I comandi riconosciuti continuano a passare per il Core.
- **Evidenza:** test OAuth con fixture sintetiche, schema dei risultati e test Python del gateway. Il consenso Google effettivo richiede un progetto abilitato e un account autorizzato.

### ADR-0094 — Client SwiftUI e ciclo di vita della sessione macOS

- **Data:** 2026-11-23.
- **Decisione:** usare SwiftUI da macOS 14, con autenticazione LocalAuthentication all’avvio e Core figlio della sessione. Un archivio marcato come gestito richiede la sessione del processo padre. Il meccanismo presume che l’account macOS e i suoi file locali siano fidati.
- **Azione rischiosa:** una sessione macOS gestita riserva al proprietario l’approvazione delle operazioni catalogate ad alto rischio o critiche. Il Core continua a verificare policy e conferme.
- **Memoria:** i formati riconoscibili di credenziali sono esclusi dalle nuove scritture della memoria, dagli episodi automatici e dal contesto dell’agente anche quando provengono da ricordi storici.
- **Distribuzione:** il DMG mantiene l’installazione per trascinamento e include uno sfondo Finder con la direzione del trascinamento. La build Windows continua a usare egui.

### ADR-0095 — Profili di risposta e contesto su richiesta in SwiftUI

- **Data:** 2026-11-23. **Stato:** implementata.
- **Decisione:** ricavare lunghezza e precisione separatamente da una finestra di dodici valutazioni per coppia provider e modello. Mostrare il risultato aggregato in Modelli AI. La valutazione resta locale e la richiesta prevale sulle preferenze.
- **Contesto:** il proprietario può includere i turni recenti compatibili per un singolo invio. Restano valide le verifiche del Core su provenienza, credenziali e budget.
- **Impostazioni:** la scelta del provider è esplicita. I modelli del catalogo possono compilare il campo prima del salvataggio e la rimozione della chiave richiede conferma.

### ADR-0184 — Risposte incomplete dei provider cloud

- **Data:** 2026-11-25. **Stato:** implementata (non ancora pubblicata).
- **Contesto:** «NVIDIA NIM ha restituito una risposta incompleta». Cause: testo scartato quando `finish_reason` era `length`; contenuto vuoto dei modelli che ragionano (token esauriti nel ragionamento); `max_tokens` fisso a 2000 per l'agente; nessun nuovo tentativo sugli errori 5xx.
- **Decisione:** il testo troncato viene conservato (`Failure::Partial`) e proseguito fino a due volte con una richiesta di continuazione, in modalità normale, in streaming e per l'agente; se resta troncato è consegnato con una nota esplicita, mai perso. Su risposta vuota o troncata senza testo si ritenta sullo stesso modello con più spazio prima del modello alternativo. L'agente parte da 4096 token (raddoppiati al secondo tentativo). `send` ritenta 500/502/503/504 con attesa crescente. Una proposta di strumento non valida è ritentata una volta; più chiamate insieme restano rifiutate (invariante a proposta singola).
- **Conseguenze:** due test esistenti aggiornati intenzionalmente (troncamento con testo → `Partial`; flusso troncato → testo più nota). Il messaggio all'utente indica cosa è già stato provato. Budget base di `Route::output_tokens` invariati.
- **Evidenza:** test del modulo `cloud` con server simulato (continuazione, nota finale, vuoto ritentato, 5xx, agente, streaming, strumento non valido). Nessuna prova contro NIM reale.

### ADR-0185 — Operazioni digitali: audit e prima fetta (risorse digitali e profili di workspace)

- **Data:** 2026-11-25. **Stato:** parziale (audit completo, due capacità implementate).
- **Contesto:** richiesta «Advanced Digital Operations & Human-Machine Interaction Upgrade» (27 aree). Audit in `docs/OPERATIONS_GAP_ANALYSIS.md`.
- **Decisione:** implementare per prime due capacità assenti, a basso rischio e senza dipendenze nuove: (1) `fs.usage` (modulo `assets`): spazio per cartella e tipo, file grandi, duplicati per SHA-256 e file temporanei, solo lettura, senza collegamenti simbolici, con limiti dichiarati; (2) profili di workspace (modulo `workspaces`, `/api/workspaces/profiles*`): applicazioni e cartelle per progetto; l'attivazione **restituisce un piano** (`app.open` con conferma) e non esegue nulla. Le altre aree restano classificate nell'audit.
- **Conseguenze:** nessuna eliminazione automatica: la pulizia resta a `fs.delete` con conferma. I profili rifiutano comandi, URL e percorsi fuori dalla workspace.
- **Evidenza:** test unitari dei due moduli, controlli e2e delle nuove rotte.

### ADR-0186 — Profili di risorse

- **Data:** 2026-11-25. **Stato:** implementata.
- **Decisione:** quattro profili (Eco, Bilanciato, Prestazioni, Massimo) scelti dal proprietario (`/api/performance`). Regolano solo limiti applicati davvero dal Core: passi dell'agente per richiesta (3/4/6/8), agenti temporanei (1/2/3/4) e intervallo delle istantanee delle metriche (15/5/2/1 min). Il predefinito «Bilanciato» coincide con i limiti precedenti (4 passi, 2 agenti, 5 min).
- **Sicurezza:** nessun profilo tocca permessi, conferme, audit o redazione. La scelta è registrata come azione del proprietario.
- **Non incluso:** limiti di CPU/RAM, scaricamento dei modelli, batteria e banda: richiedono misure dell'host non verificabili qui.
- **Evidenza:** test unitari (monotonia, persistenza, valori sconosciuti) e controlli e2e.

### ADR-0187 — Centro privacy (inventario)

- **Data:** 2026-11-25. **Stato:** parziale (API pronta, nessuna vista grafica).
- **Decisione:** `GET /api/privacy/overview` riassume solo fatti letti dallo stato reale: dimensione delle voci della cartella dati, statistiche della memoria, provider cloud con chiave (mai il valore), numero di strumenti per effetto, livello di autonomia, connettori, server MCP, osservazioni e profilo di risorse. Nessun contenuto di utente.
- **Limiti:** niente cronologia dei permessi né criteri di conservazione; la vista grafica resta da fare.
- **Evidenza:** test d'integrazione `privacy_overview_and_resource_profiles_work_through_the_api` e controllo e2e.

### ADR-0188 — Stati reali dell'avatar e mappa dell'architettura

- **Data:** 2026-11-25. **Stato:** implementata (Swift verificato solo dalla compilazione in CI; resa a schermo non vista).
- **Decisione:** l'avatar macOS aggiunge due stati legati a condizioni reali: *attesa di autorizzazione* (conferme in sospeso nel Core → colore ambra) e *offline* (Core non raggiungibile → senza saturazione e attenuato). Ogni stato ha un'etichetta di accessibilità con priorità offline > errore > parla > ascolto > elabora > attesa > inattivo. «Riduci movimento» resta rispettato. `docs/ARCHITECTURE.md` ottiene una mappa del repository e un diagramma di sequenza basati sui moduli presenti.
- **Non incluso:** stati «ricerca», «esecuzione» e «collaborazione tra agenti»: il client non riceve ancora un segnale affidabile per distinguerli dall'elaborazione generica; non si simulano.

### ADR-0189 — Mappa del codice (`code.map`)

- **Data:** 2026-11-25. **Stato:** implementata (parziale rispetto a §8).
- **Decisione:** strumento di sola lettura (rischio basso, nessuna conferma) che analizza la workspace in modo lessicale: linguaggi e righe, file più grandi, simboli (`fn`, `struct`, `def`, `class`…), dipendenze fra moduli (`use crate::`, `mod`, `import`), moduli più usati, chi dipende da un modulo (diretti e transitivi) e dove è definito un simbolo. Esclude `target`, `.git`, `node_modules` e simili; ignora collegamenti simbolici; limiti di 4000 file e 512 KiB per file.
- **Limiti dichiarati:** non è un compilatore (macro e importazioni dinamiche sfuggono); i moduli si identificano dal nome del file, quindi nomi uguali in cartelle diverse si fondono; **nessun indice persistente né incrementale**, ogni richiesta rilegge i file.
- **Evidenza:** test su una workspace sintetica e sui sorgenti del Core.

### ADR-0190 — Token di app rifiutati con 400 o 401

- **Data:** 2026-11-26. **Stato:** implementata in parte (cause ricostruite dal codice; l'errore esatto dell'utente non era noto).
- **Contesto:** «il servizio rifiuta i token di collegamento alle app» con HTTP 400 o 401. Cause trovate:
  1. **400, push:** il token FCM con spazi o a capo finali era rifiutato come non valido; quando lo stesso telefono veniva riassociato come nuovo dispositivo, il rifiuto («già associata a un altro principal») non indicava come uscirne.
  2. **401, autenticazione:** lo schema `Bearer` era riconosciuto solo con la maiuscola esatta; il token del dispositivo con spazi o a capo, oppure in esadecimale maiuscolo, era scartato o non trovato.
- **Decisione:** i token push si ripuliscono dagli spazi; il rifiuto per token già registrato da un altro dispositivo dice di revocare il vecchio dispositivo; lo schema `Bearer` non distingue maiuscole (RFC 9110); il token del dispositivo si normalizza (spazi e minuscolo) prima del controllo.
- **Scelte mantenute:** un token FCM resta legato al dispositivo che lo ha registrato, nemmeno il proprietario lo sottrae (test `owner_and_chat_device_manage_only_their_own_encrypted_tokens`); il token del proprietario è confrontato esattamente (`the_owner_token_must_match_exactly`); il codice di associazione resta monouso, valido 10 minuti e bloccato dopo 5 errori. Avevo provato a far passare il token al nuovo dispositivo e l'ho annullato perché contraddice questi test e l'isolamento voluto.
- **Non risolto per mancanza di dati:** un codice di associazione già consumato (ad esempio un nuovo invio dopo un timeout di rete) risponde ancora 401. Serve il messaggio esatto per capire se è questo il caso.
- **Evidenza:** test unitari su spazi del token push, messaggio di rifiuto e lettura delle intestazioni.

### ADR-0191 — Rimozione di Ollama Cloud

- **Data:** 2026-11-26. **Stato:** implementata.
- **Contesto:** Ollama Cloud è diventato a pagamento; il progetto non introduce servizi a pagamento senza autorizzazione.
- **Decisione:** rimosso il provider `ollama_cloud` dal Core (enum, endpoint, modelli, formato NDJSON, limiti) e dai client macOS ed egui. NVIDIA NIM resta l'unico provider cloud. All'avvio il Core cancella la chiave `cloud:ollama_cloud` e le impostazioni residue (modello e provider preferito); un identificativo `ollama_cloud` ricevuto da un client è rifiutato come provider sconosciuto. Senza chiave NIM la chat cloud non ha provider e lo dice, senza ripiegare su altri servizi.
- **Non toccato:** Ollama locale (impostazione `ollama_model` e modello di embedding del setup), escluso dal perimetro cloud. Gli ADR precedenti che citano Ollama Cloud restano come storia; questo li sostituisce.
- **Evidenza:** test aggiornati ai formati NIM, test di cancellazione della chiave e delle impostazioni, test «senza chiave non c'è provider».

### ADR-0192 — Tailscale: diagnosi del nome non trovato

- **Data:** 2026-11-26. **Stato:** implementata (non provata con una tailnet reale: qui manca Tailscale).
- **Contesto:** il collegamento mostrava «A server with the specified hostname could not be found», cioè il nome `*.ts.net` non si risolve sul dispositivo. Cause tipiche: MagicDNS disattivato nella tailnet, HTTPS non abilitato, DNS di Tailscale non in uso sul dispositivo, nome appena pubblicato.
- **Decisione:** `GET /api/tailscale` legge da `tailscale status --json` anche `MagicDNSEnabled`, i domini dei certificati, gli indirizzi privati e gli avvisi di salute, e verifica con un limite di 4 s che il nome si risolva su questo computer. Il messaggio indica il primo ostacolo reale (non connesso, MagicDNS, HTTPS, nome non risolto, già pronto) con l'azione da fare. Se un dato manca nel JSON non si inventa nulla. Il client macOS riprova fino a tre volte (3 s) la verifica di un nome `*.ts.net` e traduce gli errori di rete (nome non trovato, timeout, certificato non pronto, connessione rifiutata) in istruzioni in italiano.
- **Non fatto:** nessun ripiego sull'indirizzo IP: il certificato HTTPS vale per il nome e il Core accetta solo gli host dichiarati.
- **Evidenza:** test su `parse_details` e `advice` con stati sintetici; Swift verificato solo in CI.

### ADR-0193 — Lettura e confronto di documenti (`doc.read`, `doc.compare`)

- **Data:** 2026-11-26. **Stato:** implementata (parziale rispetto a §10).
- **Decisione:** due strumenti di sola lettura, senza dipendenze nuove (riuso di `zip_entry` e dei lettori PDF). `doc.read` estrae testo da DOCX (paragrafi in ordine, titoli come `#`, indice, conteggi, tabelle contate), PPTX (una voce per diapositiva con note del relatore), PDF con testo e file di testo (md, txt, csv, json, yaml, html, xml…). Ogni risultato porta la provenienza (percorso, dimensioni, SHA-256) ed è marcato come dato non attendibile. `doc.compare` confronta due documenti anche di formato diverso riga per riga sui primi 12 000 caratteri e dichiara se il risultato è parziale o se i file sono identici.
- **Privacy:** autore e commenti dei DOCX non si leggono; solo titolo e data di modifica.
- **Limiti dichiarati:** niente OCR, immagini e grafici ignorati, modifiche tracciate non interpretate, tabelle come paragrafi, XLSX resta a `sheet.read`, nessuna ricerca semantica fra documenti né versioni salvate.
- **Evidenza:** test con DOCX e PPTX sintetici, confronto DOCX contro Markdown, percorsi fuori dalla workspace e formati non supportati.

### ADR-0194 — Tavolozza dei comandi (⌘K) nell'app macOS

- **Data:** 2026-11-26. **Stato:** implementata (Swift verificato solo dalla compilazione in CI).
- **Decisione:** ⌘K apre «Vai a…», un elenco filtrabile (senza distinguere maiuscole né accenti) delle sezioni reali dell'app; Invio apre la prima voce. È attiva solo con il Core pronto e l'interfaccia sbloccata. Ogni voce chiama la stessa `select` della barra laterale: nessun pulsante senza funzione.
- **Non incluso:** ricerca globale su contenuti (memoria, documenti, conversazioni) e azioni rapide: richiedono un indice o endpoint di ricerca unificato.

### ADR-0195 — Apertura delle app e Spotify come unico servizio musicale

- **Data:** 2026-11-26. **Stato:** implementata (macOS e Windows non provati qui: Core su Linux).
- **Contesto:** «apri spotify» non apriva nulla. Cause trovate: il Core considerava richiesta di controllo solo `apri` seguito da una maiuscola, quindi «apri spotify» non autorizzava `app.open`; il nome non aveva ripiego (indirizzo di protocollo); l'app macOS non dichiarava `NSAppleEventsUsageDescription`, necessaria perché macOS conceda l'automazione.
- **Decisione:** tabella di app note (Spotify, Safari, Chrome, Mail, Calendario, Note, Terminale, Office, Teams, Slack, Docker, Tailscale…) riconosciute senza distinguere maiuscole e con alias italiani; su macOS `open -a <nome>` con ripiego sull'indirizzo di protocollo (`spotify:`), su Windows l'indirizzo di protocollo con ripiego sul nome. Il gateway Python riconosce le stesse frasi. Nuovo strumento `media.spotify` (stato, play, pausa, successivo, precedente), con conferma del proprietario, che non avvia Spotify e non riceve testo libero. Spotify è l'**unico** servizio musicale collegato: la scheda media di macOS non legge più Musica; `GET/POST /api/media/spotify` serve la scheda di Windows (egui), con comandi nell'audit. Su Windows lo stato si ricava dal titolo della finestra e i comandi usano i tasti multimediali.
- **Limiti:** su Windows il brano non è leggibile se la finestra di Spotify è nascosta (stato `running`); i tasti multimediali agiscono sulla sessione multimediale attiva. Nessuna prova su un Mac o su Windows reali.
- **Evidenza:** test sui nomi noti, sui piani con ripiego, sullo stato di Spotify e sulle regole del gateway.

### ADR-0196 — Ricerca web con DuckDuckGo e ripiego su Wikipedia

- **Data:** 2026-11-26. **Stato:** implementata (non provata contro DuckDuckGo reale).
- **Decisione:** il fornitore predefinito `Web` interroga la pagina HTML di DuckDuckGo (senza chiave né costi), decodifica il reindirizzamento `uddg`, scarta annunci, duplicati e indirizzi non http(s); se non risponde, chiede una verifica o non trova nulla, ripiega su Wikipedia e il risultato lo dichiara (`fallback_from`, `fallback_reason`). `JARVIS_SEARCH_PROVIDER=wikipedia` o `searxng` restano disponibili. Ogni risultato resta dato non attendibile e passa dai controlli di rete esistenti.
- **Rischio:** la struttura HTML di DuckDuckGo può cambiare e il servizio può bloccare le richieste automatiche: il ripiego è fatto per questo.

### ADR-0197 — n8n con Docker nell'interfaccia e lettura di un monitor preciso

- **Data:** 2026-11-26. **Stato:** implementata (Swift ed egui verificati solo dalla CI).
- **Decisione:** nella sezione Automazioni di macOS e di Windows c'è la scheda «n8n con Docker»: stato, abilitazione, avvio e arresto del container isolato esistente (senza rete, solo promemoria e controlli pianificati), creazione e rimozione delle routine. Nessuna modifica al Core. `screen.read` accetta `display` (1–8) e `screen.status` riporta il numero di monitor; frasi come «leggi il secondo monitor» sono riconosciute.

### ADR-0198 — Diagnosi Tailscale

- **Data:** 2026-11-26. **Stato:** implementata (non provata con una tailnet reale).
- **Contesto:** il problema persisteva con Tailscale attivo e connesso. Una causa plausibile è che questo computer non risolva il nome `*.ts.net` (Tailscale in modalità userspace, DNS di Tailscale non in uso) pur funzionando dagli altri dispositivi: la verifica da qui falliva senza dire dove.
- **Decisione:** `POST /api/tailscale/diagnose` esegue dal Core DNS → porta 443 → certificato HTTPS → risposta di JARVIS (`/api/connectivity`) e indica il primo passo che fallisce con l'azione da fare, includendo avvisi di salute e indirizzi di Tailscale. Pulsante «Diagnostica» nelle app macOS ed egui. Complementa ADR-0192.

### ADR-0199 — Memoria senza rumore, eliminazione dal menu, profili di lavoro e Hugging Face (MCP) nelle sezioni

- **Data:** 2026-11-26. **Stato:** implementata (Swift ed egui verificati solo dalla CI).
- **Memoria:** gli episodi automatici si salvano solo se valgono qualcosa: una richiesta che ha usato soltanto strumenti momentanei (ora, calcoli, meteo, stato del computer, finestre, Spotify, elenco file…) o una risposta diretta a una domanda breve non lascia più un ricordo («che ore sono» non si salva). `POST /api/memory/cleanup` (`apply: false` anteprima, `true` elimina, con audit) toglie gli episodi rumorosi già salvati; i ricordi fissati e quelli scritti dall'utente non si toccano. Il test `memory_kinds_search_graph_and_episodes` asseriva l'episodio per «che ore sono»: aggiornato di proposito.
- **Eliminazione:** nella sezione Memoria di macOS ogni ricordo ha un menu (Modifica, Fissa, Elimina) con conferma e il pulsante «Pulisci ricordi inutili»; non serve più passare dalla chat. Anche l'app Windows ha «Pulisci ricordi inutili» (l'eliminazione singola c'era già).
- **Profili di lavoro a mano:** form nella sezione Workspace (macOS ed egui): nome, descrizione, app e cartelle; `POST /api/workspaces/profiles/activate` apre le app elencate come azione esplicita del proprietario (esito per app, audit). Nessun comando.
- **Hugging Face e server MCP:** scheda nella sezione Connettori (macOS ed egui): «Collega Hugging Face» con token facoltativo, lettura degli strumenti dal server, interruttore «Senza conferma» per i soli strumenti dichiarati di sola lettura; tutti gli altri chiedono sempre conferma (ADR-0182).

### ADR-0200 — Installazione della voce da una sezione dedicata

- **Data:** 2026-11-26. **Stato:** implementata (non provata: serve rete e un computer reale).
- **Decisione:** su richiesta esplicita del proprietario, l'installazione del motore vocale locale (faster-whisper, Piper, parola di attivazione) non è più bloccata dal rifiuto «provider audio cloud»: `/api/setup` e `/api/setup/start` sono disponibili salvo `JARVIS_VOICE_SETUP=off`. Nuova sezione «Voce» in macOS (scelta del modello STT e della voce TTS, dimensioni, conferma con download stimato, avanzamento per passo); la pagina di configurazione di Windows c'era già. Resta valida la voce di sistema di macOS (ADR-0183) senza installare nulla.
- **Limiti:** i download partono solo dopo la conferma; il Core resta senza modelli di linguaggio locali (solo voce).

### ADR-0201 — Attendibilità delle fonti e notizie recenti

- **Data:** 2026-11-26. **Stato:** implementata.
- **Decisione:** ogni risultato di `web.search` riporta una valutazione euristica dal solo dominio (`alta`, `media`, `bassa`, punteggio e motivo; HTTP semplice penalizzato); le fonti a bassa attendibilità (social, forum, contenuti generati dagli utenti) vanno in coda senza essere nascoste e il riepilogo conta i livelli. Se la domanda chiede dati recenti («oggi», «ultime notizie», «questa settimana») si usa il filtro temporale di DuckDuckGo e il risultato lo dichiara. Il gateway mostra «(affidabile / da verificare / poco affidabile)» accanto a ogni fonte.
- **Limiti dichiarati:** è un indizio sul sito, non una verifica del contenuto; l'elenco dei domini è curato a mano; nessuna conferma incrociata fra fonti né data di pubblicazione letta dalle pagine.

### ADR-0202 — Mappa e orientamento

- **Data:** 2026-11-26. **Stato:** implementata (non provata contro i servizi reali; MapKit solo in CI).
- **Decisione:** strumenti di sola lettura `map.search` (luoghi, coordinate, collegamento) e `map.route` (distanza, durata, passi in italiano e tracciato; a piedi, in bici, in auto) con OpenStreetMap Nominatim e OSRM del progetto FOSSGIS, senza chiavi né costi, richieste distanziate di 1,1 s e identificativo dell'app generico. Rotte `/api/map/search` e `/api/map/route`; sezione «Mappa» in macOS con MapKit (segnaposto e percorso). Frasi come «indicazioni da Torino a Milano a piedi» o «mostrami Firenze sulla mappa» sono riconosciute. Fuori dal computer vanno solo i nomi dei luoghi.
- **Limiti:** JARVIS non conosce la posizione dell'utente (nessun GPS) e il percorso non tiene conto del traffico; i servizi pubblici hanno limiti d'uso equi. L'app Windows non ha ancora la vista grafica (restano chat e collegamento a OpenStreetMap).

### ADR-0203 — Connettore GitHub esteso: repository, file, pull request e Actions

- **Data:** 2026-11-26. **Stato:** implementata (non provata contro GitHub reale).
- **Contesto:** il proprietario vuole che JARVIS crei repository e workflow, li gestisca e li modifichi. Il connettore aveva solo 4 operazioni (elenco repository, issue, notifiche, crea issue).
- **Decisione:** il manifest GitHub passa a 25 operazioni: lettura (info repository, rami, file e cartelle, workflow, esecuzioni, pull request) e scrittura (commenti, chiusura issue, **crea repository**, modifica repository, **scrivi o aggiorna file** con un commit — anche in `.github/workflows` —, elimina file, crea ramo, **apri e unisci pull request**, **avvia, rilancia, annulla, attiva e disattiva i workflow**). Ogni scrittura richiede la conferma del proprietario; crea/modifica repository, unione di pull request ed eliminazione di file sono «critiche». `private` è obbligatorio alla creazione, così nessun repository diventa pubblico per distrazione. Non ci sono eliminazione di repository, segreti, collaboratori o amministrazione di organizzazioni.
- **Framework:** lo schema dei connettori ammette ora `PUT`, `PATCH` e `DELETE`, fino a 40 operazioni e la codifica `github_content` (testo → base64 in `content`); il parametro `file_path` mantiene le barre e rifiuta `..`; per `api.github.com` si inviano le intestazioni raccomandate (`application/vnd.github+json`, versione API).
- **Permessi del token:** tabella completa in `docs/SECURITY_AND_PRIVACY.md`. Un connettore già installato resta alla versione precedente finché non lo si reinstalla.
- **Evidenza:** test del manifest (conferma su ogni scrittura, rischio critico, assenza di operazioni distruttive di repository), dei percorsi di file e della codifica del corpo con un server locale.

### ADR-0204 — Ricerca globale e pannello Privacy

- **Data:** 2026-11-26. **Stato:** implementata (Swift verificato solo dalla CI).
- **Decisione:** `GET /api/search?q=` cerca in un solo passaggio ricordi, chat, automazioni e strumenti (al massimo 5 voci per gruppo, 2–120 caratteri) e indica la sezione dove aprire ogni risultato; solo dati già visibili al proprietario. La tavolozza ⌘K di macOS mostra le sezioni e, dopo 300 ms dall'ultimo tasto, i contenuti trovati. Nuova sezione «Privacy» (macOS) sopra `/api/privacy/overview` (ADR-0187): dati conservati e dimensioni, provider cloud con chiave, strumenti per effetto, autonomia, connettori, server MCP, osservazioni e scorciatoie alle sezioni che li gestiscono.
- **Non incluso:** ricerca nei documenti della workspace e nelle note (richiede un indice), cronologia dei permessi e criteri di conservazione.

### ADR-0205 — Immagini Docker mancanti: download con conferma

- **Data:** 2026-11-26. **Stato:** implementata (non provata con Docker reale).
- **Contesto:** avviare n8n senza l'immagine faceva partire un download implicito di centinaia di MB senza chiedere nulla, con un limite di 3 minuti.
- **Decisione:** `GET /api/n8n` dice se l'immagine è presente (`image_present`), la stima del download (`image_estimate_mb`, circa 450 MB, non misurata) e lo stato del download (`pull`: `idle`, `pulling`, `ready`, `failed`). Avviare il container senza immagine risponde `image_missing` e non scarica nulla. `POST /api/n8n {"pull": true, "running": true}` è la conferma del proprietario: scarica l'immagine in background (fino a 20 minuti) e poi avvia n8n; l'azione è nell'audit. Le app macOS e Windows mostrano «Scarica l'immagine e avvia…» con finestra di conferma, avanzamento e errore. Il sandbox del codice aveva già `sandbox.prepare`, che richiede conferma per costruire la propria immagine.
- **Limiti:** nessuna percentuale di avanzamento (Docker non la fornisce in modo stabile): si mostra il tempo trascorso; nessun catalogo generale di immagini oltre a n8n e sandbox.

### ADR-0206 — Sessioni di ricerca salvate

- **Data:** 2026-11-26. **Stato:** implementata.
- **Decisione:** nuovi strumenti locali `research.save`, `research.list` e `research.read` e rotte `/api/research` (elenco), `/api/research/{id}` (lettura con rapporto Markdown, eliminazione, audit). Una sessione ha titolo, domanda, stato (aperta o conclusa), **risultati per tipo** (verificato, non confermato, opinione, ipotesi, inferenza, obsoleto), fonti con attendibilità (ADR-0201) e domande ancora aperte. Salvando di nuovo con lo stesso `id` risultati e fonti si accumulano senza doppioni e le domande aperte si sostituiscono; «riprendi la ricerca sui database» riapre la più recente per titolo. Al massimo 60 sessioni, 40 risultati e 40 fonti ciascuna; indirizzi non http(s) scartati; contenuti con credenziali rifiutati; ciò che è salvato resta sul computer.
- **Non incluso:** salvataggio automatico di ogni ricerca (JARVIS salva quando lo ritiene utile o glielo chiedi), esportazione in file dall'app, vista grafica.
- **Evidenza:** test su accumulo, doppioni, ripresa per titolo, rapporto Markdown, rifiuti e limiti.

### ADR-0207 — Pacchetti vocali: elenco dei requisiti sempre disponibile

- **Data:** 2026-11-26. **Stato:** implementata (non provata con un'installazione reale).
- **Causa:** il passo «Pacchetti della voce» eseguiva `pip install -r <ai>/requirements-voice.txt`, ma gli installer macOS e Windows copiavano nelle risorse solo `ai/jarvis_ai`: il file con i pacchetti e gli hash non c'era e pip si fermava («file non trovato»). L'archivio `package.py` invece lo includeva, per questo il problema compariva solo con l'app installata.
- **Decisione:** (1) `release.yml` copia anche `ai/requirements-voice.txt` nelle risorse; (2) il Core incorpora il lock aggiornato alla compilazione e, se il file non è accanto al runtime AI, lo scrive in `<dati>/requirements-voice.txt` e usa quello: un'installazione già distribuita funziona anche senza rilasciare di nuovo i pacchetti. Gli hash restano obbligatori (`--require-hashes`).
- **Evidenza:** test con cartella AI vuota (usa la copia incorporata, con `--hash=sha256:`) e con file presente (ha la precedenza).

### ADR-0208 — Gmail e Google Calendar: errori leggibili, permessi verificati, richieste in italiano

- **Data:** 2026-11-26. **Stato:** implementata (non provata con un account Google reale).
- **Causa:** con l'accesso completato i connettori sembravano non funzionare perché (1) gli errori di Google si riducevano a «ha risposto 403» o «HTTP 400», senza il motivo (API Gmail/Calendar non abilitata nel progetto Cloud, permessi non concessi, client sbagliato); (2) Google consente di deselezionare i permessi nella schermata di consenso e il Core salvava comunque i token incompleti; (3) il gateway non aveva regole per «leggi le mie email» o «prossimi eventi».
- **Decisione:** (1) `service_error_detail` estrae messaggio e motivo dall'errore del servizio (mai il corpo grezzo) con il rimedio per `accessNotConfigured`, `insufficientPermissions` e quota; lo scambio dei token riporta `error_description`; (2) dopo l'accesso, `missing_scopes` confronta i permessi concessi con quelli del manifest e rifiuta l'accesso incompleto spiegando cosa rifare; (3) regole di sola lettura per posta (non lette, ricerca, lettura per id) e calendario, con riepilogo dei risultati; le scritture restano esplicite e con conferma.
- **Evidenza:** test su errore Google, scope mancanti, regole del gateway e riepilogo eventi.

### ADR-0209 — Download delle immagini Docker senza problemi di permessi

- **Data:** 2026-11-26. **Stato:** implementata (non provata con Docker reale su macOS, Windows e Linux).
- **Causa probabile:** `docker pull` usava la configurazione dell'utente: `~/.docker/config.json` con l'helper delle credenziali di Docker Desktop (non trovato col `PATH` minimo di un'app avviata dal Finder o da un servizio) o cartelle `~/.docker` non scrivibili; su Linux anche il socket senza gruppo `docker`. L'errore restava generico.
- **Decisione:** (1) il download dell'immagine pubblica usa una configurazione Docker privata e scrivibile (`<dati>/docker-config`, `config.json` vuoto) tramite `DOCKER_CONFIG`; (2) tutti i comandi Docker ampliano il `PATH` con le cartelle note di Docker; (3) `explain_pull_error` aggiunge causa e rimedio (gruppo `docker`, credenziali, cartella non scrivibile, limite del registro, rete, spazio).
- **Evidenza:** test su messaggi d'errore e creazione della configurazione privata.

### ADR-0210 — Cronologia dei permessi

- **Data:** 2026-11-26. **Stato:** implementata.
- **Contesto:** il registro di audit dice *quando* il proprietario ha cambiato permessi o autonomia, non *cosa*. Mancava una cronologia leggibile (ADR-0187, ADR-0207 la indicavano come limite).
- **Decisione:** `permission_history` registra, per ogni modifica a permessi, autonomia e arresto di emergenza, le differenze campo per campo (`field`, `from`, `to`), solo se qualcosa cambia; massimo 200 voci, 40 differenze per voce. `GET /api/permissions/history?limit=&kind=` (solo dal computer del proprietario, come i permessi). Contiene solo configurazione, nessun dato personale.
- **Limiti:** nessun criterio di conservazione configurabile e nessuna vista grafica (restano da fare).
- **Evidenza:** test su differenze, assenza di voci vuote, limite di 200 e filtro per tipo; e2e sull'API.

### ADR-0211 — Indice persistente e incrementale del codice

- **Data:** 2026-11-26. **Stato:** implementata.
- **Contesto:** `code.map` (ADR-0189) rileggeva tutti i file a ogni richiesta.
- **Decisione:** tabella `code_index` nel database locale, per workspace e percorso: dimensione, data di modifica, linguaggio, righe, simboli e importazioni. Un file con dimensione e data invariate si riusa senza rileggerlo; i nuovi o modificati si rileggono; quelli spariti escono dall'indice. L'indice si riscrive solo se cambia qualcosa e mai se la scansione è troncata. `reindex: true` lo ricostruisce. La risposta riporta `index: {reused, rescanned, removed, saved}`.
- **Sicurezza:** resta sola lettura sulla workspace; l'indice contiene solo nomi derivati dai file, nel database locale.
- **Limiti:** la data di modifica può non cambiare con strumenti che la ripristinano (usa `reindex`); analisi sempre lessicale.
- **Evidenza:** test su riuso, file modificato, file rimosso, assenza di riscrittura e risultati identici dall'indice.

### ADR-0212 — Mouse: `app.click`

- **Data:** 2026-11-26. **Stato:** implementata nel piano e nella validazione; **non provata su macOS e Windows reali**.
- **Decisione:** nuovo strumento di controllo `app.click` (`x`, `y` interi 0–16384, `clicks` 1 o 2), con richiesta esplicita del proprietario e conferma come `app.key`: rischio alto, mai ripetuto, mai in background, descrizione in chiaro della conferma. macOS: `System Events click at` (serve Accessibilità); Windows: `SetCursorPos` e `mouse_event` con i valori solo nell'ambiente del processo, mai nel testo del comando.
- **Limiti:** solo clic sinistro, nessun trascinamento né scorrimento; le coordinate sono in punti dello schermo (come le vede `screen.read`); su schermi con scala diversa possono servire verifiche. Nessuna scoperta automatica degli elementi (accessibilità ad albero) ancora.
- **Evidenza:** test su validazione (valori ostili e fuori scala), piano macOS/Windows e assenza di interpolazione.

### ADR-0213 — Aggiornamento in app: attesa della chiusura, riavvio garantito ed esito leggibile

- **Data:** 2026-11-26. **Stato:** implementata; lo script macOS è provato con strumenti simulati (successo, immagine illeggibile, ripristino), **non** su macOS e Windows reali con una release vera.
- **Sintomo:** gli aggiornamenti in app non andavano a buon fine e l'app si chiudeva senza riaprirsi.
- **Cause nello script (ADR-0109):** (1) attesa a tempo fisso di 3 secondi invece della chiusura dei processi: su macOS `open` riattivava l'app ancora in chiusura, su Windows l'installatore trovava file bloccati (Core, worker Python); (2) ogni errore (`hdiutil` che non apre l'immagine, copia o spostamento falliti) terminava lo script **senza riaprire l'app**; (3) nessun registro: impossibile capire perché.
- **Decisione:** macOS attende per PID il Core e l'app (fino a 60 s), sostituisce con ripristino della versione precedente se lo spostamento fallisce e **riapre sempre** l'app; Windows attende (fino a 40 s) e poi chiude i processi che girano dalla cartella dell'app, installa con `/S /D=<cartella>`, e riavvia sempre. Entrambi scrivono `updates/apply.log` e `updates/apply-result.txt` (`ok` o `failed: motivo`); al riavvio il Core legge l'esito una volta e, se è fallito, notifica «Aggiornamento non riuscito» con il motivo. Lo script parte staccato anche da un eventuale job di Windows (`CREATE_BREAKAWAY_FROM_JOB`, con ripiego).
- **Limiti:** se un processo non si chiude, su macOS dopo 60 s si procede comunque (la sostituzione del pacchetto è sicura anche con l'app aperta) e l'app può restare quella precedente; la notifica lo segnala al riavvio successivo. Nessuna vista dedicata dell'esito nelle app: compare nelle notifiche.
- **Evidenza:** test su script (attesa, riavvio, ripristino, percorsi non sicuri, nessun segnaposto) e sull'esito letto una sola volta; prova del script macOS con strumenti simulati.

### ADR-0214 — Tailscale: lettura dello stato resa affidabile

- **Data:** 2026-11-26. **Stato:** implementata; provata con falsi programmi Tailscale (output grande, avvisi su stderr, demone sbagliato poi giusto), **non** con una tailnet reale.
- **Contesto:** il collegamento «non andava» anche con Tailscale attivo e connesso (ADR-0141, 0192, 0198). Rileggendo `tailscale.rs` si sono trovati difetti che fanno risultare «non connesso» un client in realtà connesso.
- **Difetti e correzioni:** (1) stdout e stderr si leggevano solo a processo finito: con molti dispositivi `status --json` supera il buffer della pipe, il processo si bloccava fino ai 15 s e il JSON restava troncato → lettura in parallelo e `--peers=false` (con ripiego se il client non conosce l'opzione); (2) stdout e stderr erano concatenati, quindi un avviso come «client version != server version» rendeva il JSON non valido → flussi separati e JSON estratto con tolleranza; (3) il primo programma trovato vinceva anche se non raggiungeva il demone (riga di comando di Homebrew accanto all'app) → si prova il candidato successivo, e su macOS l'app è il primo; (4) la finestra della console compariva su Windows → `hide_console`; (5) se non c'è uno stato leggibile il messaggio riporta il motivo reale del programma invece di «non connesso».
- **Resta aperto:** cause fuori dal Core (MagicDNS o HTTPS disattivati nella console, DNS che non risolve su un altro dispositivo): la diagnosi (`/api/tailscale/diagnose`) indica il primo passo che fallisce e cosa fare.
- **Evidenza:** test su output da 400 KB senza blocco, candidati con demone irraggiungibile, JSON con avvisi e riconoscimento dell'errore.

### ADR-0215 — Viste grafiche: cronologia dei permessi, Ricerche salvate e Privacy su Windows

- **Data:** 2026-11-26. **Stato:** implementata; **verificata solo dalla CI** (Swift su macOS, egui su Windows: qui non si compilano).
- **Decisione:** `/api/privacy/overview` include `permission_history` (ultime 10 voci, ADR-0210). macOS: card «Cronologia dei permessi» nella sezione Privacy e nuova sezione «Ricerche» (elenco, apertura del rapporto Markdown, eliminazione con `DELETE /api/research/{id}`). Windows (egui): nuove pagine «Privacy» (con la stessa cronologia) e «Ricerche» (elenco generico). Nessuna nuova rotta.
- **Non incluso:** stati dell'avatar per ricerca, esecuzione e collaborazione: il Core è occupato mentre lavora e non espone un canale di attività in corso, quindi servirebbe un canale di eventi dedicato; resta aperto. Visione dello schermo oltre l'OCR e albero di accessibilità (rilevazione degli elementi per `app.click`) restano aperti.
- **Evidenza:** test API sulla presenza della cronologia nel riepilogo; formattazione `cargo fmt --check` dell'app; compilazione in CI.

### ADR-0216 — Elementi dell'interfaccia: `app.elements`

- **Data:** 2026-11-26. **Stato:** implementata nel piano, nel parsing e nella validazione; **non provata su macOS e Windows reali**.
- **Contesto:** `app.click` (ADR-0212) richiede coordinate; senza sapere dove sono i controlli l'agente doveva indovinare.
- **Decisione:** strumento di controllo `app.elements` (sola lettura, rischio alto, conferma richiesta, come `screen.read`): macOS con Accessibilità (System Events, `entire contents`, 400 elementi al massimo), Windows con UI Automation sulla finestra in primo piano. Restituisce ruolo, nome, posizione, dimensione e **centro** da usare con `app.click`; massimo 80 elementi; **campi password esclusi** (`AXSecureTextField`, `IsPassword`), elementi senza dimensione o fuori scala e senza nome scartati. Nessun valore digitato viene letto.
- **Limiti:** `entire contents` può essere lento in finestre complesse (timeout 40 s); su Windows le coordinate sono quelle del processo (scala dello schermo) e vanno verificate su schermi ad alta densità; nessuna scoperta di menu di sistema o altre finestre.
- **Evidenza:** test su parsing (password escluse, centri, rumore), piano per i due sistemi e classificazione come strumento di controllo; regola del gateway («quali pulsanti ci sono nella finestra»).

## Aggiornamento del registro

Per un nuovo ADR registrare identificativo, data, stato, contesto, opzioni, decisione, motivazione, conseguenze, evidenza e documenti interessati. Indicare esplicitamente quando una nuova scelta sostituisce una precedente. Se si recupera una risposta storica, collegare la fonte e verificare domanda, scelta e precisazioni prima di aggiungerla.
