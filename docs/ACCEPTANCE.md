# Accettazione di JARVIS (Fasi 6 e 7)

Questo documento raccoglie ciò che serve per l'accettazione finale descritta in [DEVOPS_QA_OBSERVABILITY.md](DEVOPS_QA_OBSERVABILITY.md): matrice requisiti–verifiche, controlli automatici, prove da fare sul computer del proprietario e criteri di rilascio. Lo stato dei singoli requisiti è in [REQUIREMENTS.md](REQUIREMENTS.md#tracciabilità). Il client iOS e l'interfaccia Web/PWA sono ritirati; le prove usano il desktop egui o l'app Android.

## Matrice requisiti–verifiche

| ID | Verifica automatica (CI) | Prova del proprietario | Esito automatico | Limite noto |
|---|---|---|---|---|
| RF-01 Conversazione | `core/server/tests/api.rs`, E2E | P1, P2 | Verde | Qualità con modelli reali da valutare |
| RF-02 Orchestrazione | `api.rs`, `agent_loop.rs`, `missions.rs`, `orchestrator/mission_runner.rs`, contratti task | P18 | Verde | Ciclo fino a 4 strumenti per richiesta (ADR-0037); missioni autonome in background con strumenti senza conferma (ADR-0096); il ciclo guidato dal modello cloud va collaudato con una chiave reale |
| RF-03 Agenti | contratti delega, `agents.rs`, `agent_loop.rs`, `subagents.rs`, `skills.rs`, `tests/ai` | P17 | Verde | 5 specializzazioni con ambito di strumenti (ADR-0038); agenti temporanei di sola lettura, un livello (ADR-0040); competenze del proprietario (ADR-0048) |
| RF-04 AI Gateway | `tests/ai/test_gateway.py`, guasti in `resilience.rs` | P2 | Verde | Solo backend locali |
| RF-05 Computer Use | strumenti file, sistema, processi e terminale; `api.rs`, `terminal.rs` | P3, P14 | Verde | Terminale con conferma a ogni comando (ADR-0038); niente controllo di desktop e mouse |
| RF-06 Web | `web.rs` (SSRF, iniezioni), `browser.rs` (`tests/browser.rs`), E2E | P4, P15 | Verde | Navigazione con il browser del sistema (ADR-0043); niente sessioni con le credenziali del proprietario |
| RF-07 Automazioni | `automation.rs`, `proactive.rs`, E2E | P5, P19 | Verde | Iniziative proattive locali: briefing, disco, conferme, automazioni in errore (ADR-0097) |
| RF-08 Memoria | `api.rs`, `memory.rs`, E2E | P1 | Verde | Sinonimi con gli embedding di Ollama (ADR-0028); nessun PostgreSQL |
| RF-09 Voce | `voice.rs`, `tests/ai/test_voice.py`, E2E con motori finti | P6 | Verde (finti) | Motori reali solo sul tuo computer |
| RF-10 Client | E2E API; CI desktop macOS/Windows e Core headless Linux | P7, P8b | In corso | Test su dispositivi reali e parità UI ancora da completare |
| RF-11 Dispositivi e nodi | `devices.rs`, `nodes.rs`, `node_tls.rs`, E2E | P8, P13 | Verde | Scelta automatica (ADR-0031); mTLS con certificati fissati (ADR-0041); VPN mesh affidata a Tailscale |
| RF-12 Dati | backup, rollback | P9 | Verde | Nessuna sincronizzazione fra nodi |
| RF-13 Notifiche | `notifications.rs`, `channels.rs`, E2E | P5 | Verde | Canali esterni ntfy e Telegram (ADR-0026) |
| RF-14 Connettori | `connectors.rs`, `oauth.rs`, `connector_catalog.rs`, E2E | P10, P16 | Verde | OAuth 2.0 con PKCE (ADR-0027); catalogo Gmail, Calendar, Outlook, GitHub, Todoist, Slack (ADR-0047); niente plugin con codice |
| RF-15 Miglioramenti | — | — | Non iniziato | Fuori dalla prima release |
| RNF-01 Contratti | `tests/contracts`, `jarvis-contracts` | — | Verde | — |
| RNF-02 Guasti | `resilience.rs` (blocco, crash, output non valido) | — | Verde | Perdita di rete fra nodi non provata |
| RNF-03 Risorse | `tests/perf` (p95 in CI) | P11 | Verde | Misure con modelli reali da fare |
| RNF-04 Sicurezza | token, host, CSP, policy, conferme, audit, revoca, passkey (`passkeys.rs`) | P8, P12 | Verde | Passkey per le approvazioni dai dispositivi (ADR-0042); mTLS dei nodi con `JARVIS_NODE_TLS_ADDR` (ADR-0041); portachiavi su macOS, Windows e Linux con sessione grafica |
| RNF-05 Local-first | nessun backend cloud | — | Verde | — |
| RNF-06 Offline | backend offline in tutta la CI | P2 | Verde | — |
| RNF-07 Osservabilità | metriche, log, esportazione, tracce dei task (`api.rs`) | — | Verde | Tracce W3C locali per task (ADR-0039); nessun esportatore OTLP |
| RNF-08 Backup e rollback | `backup.rs`, `rollback.rs` | P9 | Verde | Firma verificata in CI; aggiornamento in-place da collaudare |

## Controlli automatici a ogni modifica

Workflow `.github/workflows/contracts.yml`:
- contratti e runtime AI (Python);
- Core: fmt, clippy e test;
- E2E API Core in modalità offline, senza browser;
- app desktop su macOS e Windows;
- Core headless avviato su Linux, macOS e Windows, con misura delle latenze;
- audit delle dipendenze con `cargo audit` e `pip-audit`.

Il workflow `.github/workflows/release.yml` costruisce e installa il DMG macOS Apple Silicon e il setup Windows x64, poi verifica l'avvio del Core installato; il job Android produce l'APK release firmato.

## Prove del proprietario

Da eseguire sul proprio computer con una release o con i sorgenti. Annotare esito e data.

| # | Prova | Come | Atteso |
|---|---|---|---|
| P1 | Primo avvio desktop | `./jarvis.sh desktop` su macOS, `.\jarvis.ps1 desktop` su Windows o app installata | La finestra nativa si apre e avvia il Core; chat, conferme e memoria funzionano |
| P2 | Inferenza cloud | Configura un provider in un ambiente di test e invia una richiesta sintetica | La risposta indica il provider e il modello usato |
| P3 | File | «scrivi prova.txt: ciao», approva, «leggi prova.txt» | Conferma richiesta; file in `data/workspace` |
| P4 | Web reale | «cerca Rust linguaggio», poi «apri <link>» | Fonti con link, ora e SHA-256 |
| P5 | Automazioni e notifiche | «tra 2 minuti che ore sono» | Esecuzione all'ora prevista e notifica nella campanella |
| P6 | Voce | Sospesa fino all'implementazione dei controlli nei client nativi | Backend API e motori restano disponibili; trascrizione, TTS e wake word non sono esposti dalla UI desktop egui |
| P7 | App desktop | Installa il DMG macOS o setup EXE Windows; dai sorgenti usa il launcher del sistema supportato | Si apre senza installare Python o Rust; le nuove notifiche compaiono come notifiche OS quando la finestra non è in primo piano; chiudendo la finestra l'app termina; i dati restano nella cartella utente |
| P8 | Telefono Android | Configura HTTPS e `JARVIS_ALLOWED_HOSTS`, genera il codice dal desktop e inseriscilo nell'app Android | Associazione, chat e revoca immediata |
| P8b | Push Android | Installa l'APK release firmato, abilita permessi e consenso push, invia una notifica sintetica con l'app in background | La notifica arriva genericamente senza contenuto privato; la prova reale richiede Firebase configurato |
| P9 | Backup e rollback | Sezione Sistema → backup; `./jarvis.sh restore <file>`; dopo un aggiornamento `./jarvis.sh rollback` | Dati ripristinati; i dati sostituiti restano in una cartella a parte |
| P10 | Connettore | Sospesa finché non è completata la UI nativa guidata | Le API del Core restano disponibili; la schermata desktop è ancora generica |
| P11 | Prestazioni | `python tests/perf/latency.py http://127.0.0.1:8765 30` con Ollama attivo | p95 entro `tests/perf/slo.json` per le serie senza modello; registrare quelle con il modello |
| P12 | Sicurezza | Dal telefono prova sezioni del proprietario; apri l'indirizzo da un host non ammesso | Accesso negato; host rifiutato |
| P13 | Nodo esecutore | Su un secondo computer con Tailscale: codice «Nodo esecutore» dalla sezione Dispositivi, `./jarvis.sh node https://core.example.invalid <codice>`, poi in chat «nodo <nome> system.info» e «nodo <nome> fs.write path=prova.txt content=ciao»; infine revoca | Informazioni del secondo computer; scrittura solo dopo conferma e nella workspace del nodo; dopo la revoca il nodo si ferma e i suoi strumenti spariscono |
| P14 | Processi e terminale | «quali processi usano più memoria?», poi «terminale: ls» e approva; prova «terminale: sudo ls» | Elenco dei processi senza conferma; il comando chiede conferma e mostra l'output; `sudo` viene negato |
| P15 | Navigazione Web | Con Chrome o Edge installato: «naviga https://it.wikipedia.org e clicca Storia», poi «compila modulo <pagina con un modulo di prova>: campo=valore» e approva | Testo della pagina seguita con fonte; il modulo chiede conferma, mostra i campi e viene inviato solo dopo l'approvazione |
| P16 | Gmail e Calendar | Sospesa fino al completamento della UI nativa guidata e alla configurazione del provider | Le API e il catalogo esistono; non verificato il flusso OAuth dal client nativo |
| P17 | Competenze | In Competenze crea «Viaggi» con attivazione «itinerario», istruzioni «proponi un itinerario giorno per giorno» e strumento `weather.forecast`; in chat: «prepara un itinerario per Lisbona» | Passo «competenza · Viaggi», risposta che segue le istruzioni con il modello locale; disattivata, non si attiva più |
| P18 | Missioni autonome | In Modelli AI salva una chiave cloud; in Missioni crea «controlla le novità su Rust e riassumile» con ripetizione ogni 60 minuti, oppure in chat «avvia una missione: …» e approva | Dopo al massimo un giro dello scheduler arriva una notifica «Missione, rapporto» con fatti e fonti; il diario registra gli strumenti usati; Pausa, Annulla ed Esegui ora funzionano; senza chiave la missione usa solo i comandi locali |
| P19 | Iniziative proattive | In Missioni → Iniziative imposta il briefing fra due minuti e disattiva il silenzio; aggiungi una cosa da fare scaduta e premi «Briefing adesso» | Arriva «Buongiorno: …» con le cose da fare, le conferme e le notifiche reali; il briefing automatico arriva una sola volta al giorno e mai nelle ore di silenzio |

## Criteri di rilascio

Una release si pubblica quando:
1. tutti i job delle due CI sono verdi sul commit da rilasciare;
2. le prove applicabili sono superate o i difetti aperti (incluse le lacune UI native) sono registrati e accettati come non bloccanti;
3. il proprietario accetta esplicitamente.

Restano decisioni del proprietario:
- VPN mesh;
- evoluzione dell'avatar con sincronizzazione labiale basata sui fonemi e client nativo Android.

## Procedura di rilascio e rollback

Il test `tests/contracts/test_packaging.py` verifica gli archivi ZIP e TAR.GZ e le loro impronte con un binario sintetico. Il confezionamento normalizza ordine, date e permessi. A parità di file di input, binario e runtime di compressione, i metadati del filesystem non cambiano l'archivio.

**Rilascio:**
1. Aggiornare la versione in `core/server/Cargo.toml`.
2. Unire la modifica con CI verde.
3. Su richiesta del proprietario si crea il tag `vX.Y.Z` (con `git push` del tag oppure avviando a mano il workflow «release» con l'input `tag`, che crea tag e release sul commit costruito e rifiuta un tag diverso dalla versione del Core): il workflow costruisce e verifica il setup Windows, il DMG macOS e l'APK Android; installa i pacchetti desktop sui runner e avvia il Core incluso. Pubblica i tre artefatti con `SHA256SUMS`, firme Sigstore e attestazioni di provenienza. Dopo una release stabile resta pubblicata solo la release più recente e il suo tag. Gli archivi standalone del Core creati da `scripts/package.py` restano disponibili per l'uso da terminale ma non vengono pubblicati. I tag con suffisso (`-rc.1`) diventano prerelease.

**Verifica di un archivio:**

```bash
sha256sum -c jarvis-X.Y.Z-linux-x86_64.tar.gz.sha256
```

**Rollback:**
1. Chiudere JARVIS.
2. Eseguire `./jarvis.sh rollback` (elenco delle copie: `./jarvis.sh rollback --list`).
3. Avviare la release precedente.

All'avvio di una versione diversa, il Core copia automaticamente i dati in `<dati>-rollback/` prima di aprirli e conserva le ultime 3 copie.
