# Accettazione di JARVIS (Fasi 6 e 7)

Questo documento raccoglie ciò che serve per l'accettazione finale descritta in [DEVOPS_QA_OBSERVABILITY.md](DEVOPS_QA_OBSERVABILITY.md): matrice requisiti–verifiche, controlli automatici, prove da fare sul computer del proprietario e criteri di rilascio. Lo stato dei singoli requisiti è in [REQUIREMENTS.md](REQUIREMENTS.md#tracciabilità); qui c'è la sintesi per decidere.

## Matrice requisiti–verifiche

| ID | Verifica automatica (CI) | Prova del proprietario | Esito automatico | Limite noto |
|---|---|---|---|---|
| RF-01 Conversazione | `core/server/tests/api.rs`, E2E | P1, P2 | Verde | Qualità con modelli reali da valutare |
| RF-02 Orchestrazione | `api.rs`, `agent_loop.rs`, contratti task | — | Verde | Ciclo fino a 4 strumenti per richiesta (ADR-0037) |
| RF-03 Agenti | contratti delega, `agents.rs`, `agent_loop.rs`, `subagents.rs`, `tests/ai` | — | Verde | 5 specializzazioni con ambito di strumenti (ADR-0038); agenti temporanei di sola lettura, un livello (ADR-0040) |
| RF-04 AI Gateway | `tests/ai/test_gateway.py`, guasti in `resilience.rs` | P2 | Verde | Solo backend locali |
| RF-05 Computer Use | strumenti file, sistema, processi e terminale; `api.rs`, `terminal.rs` | P3, P14 | Verde | Terminale con conferma a ogni comando (ADR-0038); niente controllo di desktop e mouse |
| RF-06 Web | `web.rs` (SSRF, iniezioni), `browser.rs` (`tests/browser.rs`), E2E | P4, P15 | Verde | Navigazione con il browser del sistema (ADR-0043); niente sessioni con le credenziali del proprietario |
| RF-07 Automazioni | `automation.rs`, E2E | P5 | Verde | — |
| RF-08 Memoria | `api.rs`, `memory.rs`, E2E | P1 | Verde | Sinonimi con gli embedding di Ollama (ADR-0028); nessun PostgreSQL |
| RF-09 Voce | `voice.rs`, `tests/ai/test_voice.py`, E2E con motori finti | P6 | Verde (finti) | Motori reali solo sul tuo computer |
| RF-10 Client | E2E Web e telefono simulato, CI desktop su 3 sistemi | P7, P8 | Verde | iOS e Android nativi compilati in CI; prove su dispositivo (P8a, P8b) e sincronizzazione fonetica in sospeso |
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
| RNF-08 Backup e rollback | `backup.rs`, `rollback.rs` | P9 | Verde | Installatori firmati assenti |

## Controlli automatici a ogni modifica

Workflow `.github/workflows/contracts.yml`:
- contratti e runtime AI (Python);
- Core: fmt, clippy e test;
- E2E dell'interfaccia con un telefono simulato;
- app desktop su Linux, macOS e Windows;
- avvio reale con i lanciatori sui 3 sistemi, con misura delle latenze;
- audit delle dipendenze con `cargo audit` e `pip-audit`.

Il workflow `.github/workflows/release.yml` costruisce gli archivi di release sui 3 sistemi e prova l'avvio di ciascun archivio senza Rust.

## Prove del proprietario

Da eseguire sul proprio computer con una release o con i sorgenti. Annotare esito e data.

| # | Prova | Come | Atteso |
|---|---|---|---|
| P1 | Primo avvio | `./jarvis.sh` (Windows: `jarvis.cmd`) | Il browser si apre; «ciao», «ricorda che …», «cosa ricordi» funzionano |
| P2 | Modello locale | Installa Ollama, `ollama pull qwen2.5:3b`, riavvia | L'intestazione mostra Ollama; la conversazione è libera; spegnendo Ollama JARVIS passa a offline e lo dichiara; riavviando Ollama, dopo almeno 15 secondi dall'ultimo guasto la richiesta successiva torna a usare il modello |
| P3 | File | «scrivi prova.txt: ciao», approva, «leggi prova.txt» | Conferma richiesta; file in `data/workspace` |
| P4 | Web reale | «cerca Rust linguaggio», poi «apri <link>» | Fonti con link, ora e SHA-256 |
| P5 | Automazioni e notifiche | «tra 2 minuti che ore sono» | Esecuzione all'ora prevista e notifica nella campanella |
| P6 | Voce | Dall'app installata: al primo avvio «Configura JARVIS» → Installa (dai sorgenti anche `./jarvis.sh voice-setup`), poi 🎤 e «Mani libere» | Trascrizione corretta in italiano; «Ehi Jarvis» attiva; interruzione con la voce |
| P7 | App desktop | Installa dalla release (`.msi`/`.exe`, `.dmg`, `.deb`/`.AppImage`) e apri JARVIS dal menu; dai sorgenti: `./jarvis.sh desktop` | Si apre senza installare Python o Rust; chiudendo la finestra JARVIS resta nella barra dei menu o nell'area di notifica (⌘⇧J / Ctrl+Maiusc+J la riapre), «Esci da JARVIS» ferma Core e worker; le notifiche arrivano come notifiche del sistema; la disinstallazione rimuove l'app ma non i dati |
| P8 | Telefono | `tailscale serve 8765`, `JARVIS_ALLOWED_HOSTS=<host>`, codice dalla sezione Dispositivi | Associazione, chat dal telefono; dopo la revoca l'accesso cade subito |
| P8a | App nativa iOS | Avvia il target JARVIS da Xcode, associa al Core, invia un messaggio, verifica conferme con e senza permesso, riapri l’app e revoca dal Core | Cronologia e chat funzionano, l’associazione persiste e la prima richiesta successiva alla revoca torna all’associazione |
| P8b | App nativa Android | Installa l'APK `jarvis-android-debug` della CI (o compila `apps/android`), inserisci l'indirizzo HTTPS del Core e il codice, invia un messaggio, verifica le conferme con e senza permesso, riapri l'app e revoca dal Core | Chat e cronologia funzionano, l'associazione resta dopo il riavvio dell'app, dopo la revoca l'app torna all'associazione |
| P9 | Backup e rollback | Sezione Sistema → backup; `./jarvis.sh restore <file>`; dopo un aggiornamento `./jarvis.sh rollback` | Dati ripristinati; i dati sostituiti restano in una cartella a parte |
| P10 | Connettore | Installa un connettore verso un servizio tuo, autorizza, revoca | Chiamata riuscita; dopo la revoca la chiamata è negata |
| P11 | Prestazioni | `python tests/perf/latency.py http://127.0.0.1:8765 30` con Ollama attivo | p95 entro `tests/perf/slo.json` per le serie senza modello; registrare quelle con il modello |
| P12 | Sicurezza | Dal telefono prova sezioni del proprietario; apri l'indirizzo da un host non ammesso | Accesso negato; host rifiutato |
| P13 | Nodo esecutore | Su un secondo computer con Tailscale: codice «Nodo esecutore» dalla sezione Dispositivi, `./jarvis.sh node https://<core>.ts.net <codice>`, poi in chat «nodo <nome> system.info» e «nodo <nome> fs.write path=prova.txt content=ciao»; infine revoca | Informazioni del secondo computer; scrittura solo dopo conferma e nella workspace del nodo; dopo la revoca il nodo si ferma e i suoi strumenti spariscono |
| P14 | Processi e terminale | «quali processi usano più memoria?», poi «terminale: ls» e approva; prova «terminale: sudo ls» | Elenco dei processi senza conferma; il comando chiede conferma e mostra l'output; `sudo` viene negato |
| P15 | Navigazione Web | Con Chrome o Edge installato: «naviga https://it.wikipedia.org e clicca Storia», poi «compila modulo <pagina con un modulo di prova>: campo=valore» e approva | Testo della pagina seguita con fonte; il modulo chiede conferma, mostra i campi e viene inviato solo dopo l'approvazione |
| P16 | Gmail e Calendar | Crea un ID client OAuth «App desktop» in Google Cloud (Gmail API e Calendar API abilitate), poi in Connettori → Catalogo aggiungi Gmail e Google Calendar e premi «Accedi»; in chat: «connettore gmail cerca q=is:unread», «connettore google_calendar eventi timeMin=<oggi>» | Elenco reale delle email e degli eventi; invio di una bozza o di un evento solo dopo la conferma |

## Criteri di rilascio

Una release `v1.0.0` si pubblica quando:
1. tutti i job delle due CI sono verdi sul commit da rilasciare;
2. le prove P1–P16, incluse P8a e P8b per i client nativi, sono superate o i difetti aperti sono registrati come non bloccanti;
3. il proprietario accetta esplicitamente.

Restano decisioni del proprietario, perché richiedono suoi account, certificati o scelte:
- firma dei binari e installatori;
- VPN mesh;
- evoluzione dell'avatar con sincronizzazione labiale basata sui fonemi e client nativo Android.

## Procedura di rilascio e rollback

Il test `tests/contracts/test_packaging.py` verifica gli archivi ZIP e TAR.GZ e le loro impronte con un binario sintetico. Il confezionamento normalizza ordine, date e permessi. A parità di file di input, binario e runtime di compressione, i metadati del filesystem non cambiano l'archivio.

**Rilascio:**
1. Aggiornare la versione in `core/server/Cargo.toml`.
2. Unire la modifica con CI verde.
3. Su richiesta del proprietario si crea il tag `vX.Y.Z` (con `git push` del tag oppure avviando a mano il workflow «release» con l'input `tag`, che crea tag e release sul commit costruito e rifiuta un tag diverso dalla versione del Core): il workflow di release costruisce i programmi di installazione (Windows `.msi` e setup `.exe`, macOS `.dmg`, Linux `.deb` e `.AppImage`) con Core, runtime AI e Python autonomo inclusi. Li installa davvero sul runner (`.deb` e `.msi`; su macOS copia l'app dal `.dmg` montato), avvia il Core installato con la prova di avvio e pubblica la release con l'elenco `SHA256SUMS`, firmato con Sigstore, e un'attestazione di provenienza per ogni installer (ADR-0020). Dopo una release stabile restano pubblicati solo quella release e il suo tag. Gli archivi di `scripts/package.py` restano disponibili per l'uso da terminale ma non vengono pubblicati. I tag con suffisso (`-rc.1`) diventano prerelease.

**Verifica di un archivio:**

```bash
sha256sum -c jarvis-X.Y.Z-linux-x86_64.tar.gz.sha256
```

**Rollback:**
1. Chiudere JARVIS.
2. Eseguire `./jarvis.sh rollback` (elenco delle copie: `./jarvis.sh rollback --list`).
3. Avviare la release precedente.

All'avvio di una versione diversa, il Core copia automaticamente i dati in `<dati>-rollback/` prima di aprirli e conserva le ultime 3 copie.
