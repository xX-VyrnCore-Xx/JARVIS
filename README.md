# JARVIS — Autonomous Personal AI Ecosystem

JARVIS è un ecosistema AI privato, autonomo, modulare e multipiattaforma per un singolo proprietario. Un Core Linux coordina modelli, agenti, memoria e strumenti. I dispositivi autorizzati possono eseguire attività locali e riprendere la sincronizzazione al ritorno della connessione.

L'inferenza e la persistenza privilegiano l'ambiente locale. Il cloud interviene quando serve una capacità superiore e la policy autorizza la trasmissione. L'autonomia comprende pianificazione, esecuzione e verifica, entro i permessi del proprietario.

## Prova il prototipo (v0.1)

**Con il programma di installazione (consigliato):** dalla pagina Releases di GitHub scarica il file per il tuo sistema:
- **Windows:** `JARVIS_<versione>_x64_en-US.msi` oppure `JARVIS_<versione>_x64-setup.exe`;
- **macOS** (chip Apple): `JARVIS_<versione>_aarch64.dmg`;
- **Linux:** `.deb` per Debian/Ubuntu oppure `.AppImage`.

Installalo e apri **JARVIS** dal menu delle applicazioni. Core, runtime AI e Python sono inclusi: non serve installare altro. L'impronta si verifica con il file `.sha256` accanto (`sha256sum -c <file>.sha256`).

I file non sono firmati:
- **Windows:** SmartScreen può chiedere «Ulteriori informazioni → Esegui comunque»;
- **macOS:** apri l'app con clic destro → Apri.

Le istruzioni qui sotto valgono per chi usa i sorgenti.

Il prototipo funziona sul tuo computer, senza cloud. Comprende chat, strumenti con permessi, conferme, memoria, workspace e audit.

**Requisiti:** Windows, macOS o Linux con [Python 3.10+](https://www.python.org/) (su Windows spunta «Add to PATH»), [Rust](https://rustup.rs/) e Git. Opzionale: [Ollama](https://ollama.com) per la conversazione libera con un modello locale.

```bash
git clone https://github.com/xX-VyrnCore-Xx/JARVIS.git
```

Avvio:
- **Linux e macOS:** nel terminale, `cd JARVIS && ./jarvis.sh`. Se compare «Permission denied», per esempio dopo aver scaricato lo ZIP, usa `bash jarvis.sh`.
- **Windows:** doppio clic su **`jarvis.cmd`**, oppure in PowerShell `.\jarvis.ps1`.

Al primo avvio vengono compilati i componenti: richiede alcuni minuti e l'avanzamento compare nel terminale. Poi il browser si apre da solo su **http://localhost:8765**. Il terminale deve restare aperto finché usi JARVIS; per chiuderlo premi Ctrl+C.

**Se non parte:** `./jarvis.sh doctor` (Windows: `jarvis.cmd doctor`) controlla Python, Rust, compilatore C e porta, e indica cosa installare. Se la porta 8765 è occupata, avvia con `JARVIS_PORT=8766 ./jarvis.sh`.

**App desktop (Tauri):** con `./jarvis.sh desktop` JARVIS si apre in una finestra sua. L'app avvia il Core da sola, tiene i dati nella cartella dell'applicazione dell'utente e alla chiusura spegne tutto. Su Linux servono le librerie WebKitGTK:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev
```

- **Senza Ollama** JARVIS parte in modalità *offline*: un modello a regole capisce comandi come «che ore sono», «info sistema», «elenca file», «scrivi note.txt: testo», «leggi note.txt», «ricorda che …», «cosa ricordi» e «dimentica <id>».
- **Con Ollama** installa un modello con supporto agli strumenti (per esempio `ollama pull qwen2.5:3b`) e invia un messaggio. Se il gateway è già in modalità offline, attendi almeno 15 secondi dall'ultimo tentativo: la richiesta successiva verifica di nuovo Ollama.

- **Automazioni**: scrivi in chat «ogni ora info sistema», «ogni giorno alle 08:00 scrivi buongiorno.txt: ciao» o «tra 5 minuti che ore sono», oppure usa la sezione Automazioni. Le automazioni sopravvivono al riavvio e conservano la cronologia di ogni esecuzione. Si possono modificare («modifica automazione <id> ogni 2 ore» o il pulsante Modifica): ogni modifica crea una nuova versione, approvata da te.

- **Ricerca Web**: «cerca intelligenza artificiale» (Wikipedia, oppure il tuo SearXNG) e «apri https://…». Sotto ogni risposta compaiono le **fonti**: link, ora di recupero e impronta SHA-256 della pagina letta. Le istruzioni contenute nelle pagine non vengono mai eseguite. Gli indirizzi locali e della rete di casa sono bloccati.
- **Voce** (tutto locale): `./jarvis.sh voice-setup` installa una volta faster-whisper, Piper e openWakeWord e scarica i modelli (circa 500 MB). Poi:
  - premi 🎤 per parlare e premilo di nuovo per inviare;
  - attiva «Risposte vocali» per farti leggere le risposte (⏹ o il microfono le interrompono);
  - attiva «Mani libere» per dire «Ehi Jarvis, …» senza toccare nulla.

  L'audio non lascia il computer e non viene salvato. Le conferme restano un clic.
- **Notifiche**: la campanella in alto segnala conferme in attesa, automazioni fallite e anomalie (agente AI non disponibile, audit alterato). Ogni notifica è collegata al task che l'ha generata.

- **Backup**: dalla sezione Sistema o con `./jarvis.sh backup` crei una copia cifrata dei tuoi dati, protetta da una passphrase che scegli tu (senza passphrase la copia non si apre). Per ripristinarla, con JARVIS chiuso: `./jarvis.sh restore data-backups/<file>.jbak`. Dopo un aggiornamento, `./jarvis.sh rollback` riporta i dati alla copia fatta automaticamente prima del cambio di versione.
- **Connettori**: nella sezione Connettori incolli il manifest JSON di un servizio esterno (esempio in `tests/contracts/fixtures/valid/connector.meteo.json`), lo autorizzi con il suo token e lo usi in chat con «connettore <id> <operazione> chiave=valore». Il token resta cifrato nel Core e non viene mai mostrato. Le operazioni che scrivono verso l'esterno chiedono sempre conferma. «Revoca» cancella il token e blocca subito le chiamate.
- **Dal telefono**: pubblica JARVIS nella tua rete privata con un proxy HTTPS (per esempio [Tailscale](https://tailscale.com): `tailscale serve 8765`), avvialo con `JARVIS_ALLOWED_HOSTS=<nome del computer nella rete Tailscale>`, apri l'indirizzo sul telefono e inserisci il codice generato nella sezione **Dispositivi**. Puoi installarlo come app dalla schermata Home. Ogni dispositivo si revoca con un clic, con effetto immediato.
- **App iOS nativa:** apri `apps/ios/JARVIS.xcodeproj` in Xcode. Il client per iOS/iPadOS 17+ si associa allo stesso Core HTTPS della PWA e offre chat, cronologia e conferme secondo i permessi del dispositivo. Vedi [Interfacce e dispositivi](docs/INTERFACE_AND_DEVICES.md).
- **Altri computer (nodi esecutori)**: nella sezione Dispositivi spunta «Nodo esecutore» e genera un codice. Sull'altro computer, nella cartella di JARVIS, esegui `./jarvis.sh node <indirizzo del Core> <codice>` (Windows: `jarvis.cmd node <indirizzo> <codice>`). Poi in chat: «nodo <nome> system.info» o «nodo <nome> fs.list path=.». Le scritture chiedono conferma e la revoca ferma subito il nodo.
- **Sistema**: la sezione omonima mostra stato dei componenti, latenze misurate e lo storico delle ultime 24 ore (conservato 7 giorni), esportabile in JSON o nel formato di Prometheus. I log diagnostici sono in `data/logs/core.jsonl` e non contengono i tuoi messaggi.

Cosa osservare:
- ogni azione mostra agente, strumento, decisione di policy ed esito;
- scrivere un file o cancellare un ricordo richiede la tua conferma, valida una sola volta e per 5 minuti;
- i file restano nella cartella `data/workspace`, e ogni tentativo di uscirne viene negato;
- le sezioni Attività, Memoria e Audit mostrano stato, ricordi cancellabili e la catena di hash verificata.

| Variabile | Predefinito | Effetto |
|---|---|---|
| `JARVIS_PORT` | `8765` | Porta locale (il Core ascolta solo su 127.0.0.1) |
| `JARVIS_DATA_DIR` | `./data` | Database, workspace e audit. Per ripartire da zero cancella questa cartella |
| `JARVIS_AI_BACKEND` | `auto` | `auto`, `ollama` oppure `offline` |
| `JARVIS_OLLAMA_URL` | `http://127.0.0.1:11434` | Indirizzo di Ollama |
| `JARVIS_OLLAMA_MODEL` | primo modello installato | Modello da usare, per esempio `qwen2.5:3b` |
| `JARVIS_SCHEDULER_TICK_S` | `20` | Ogni quanti secondi il Core controlla le automazioni scadute |
| `JARVIS_WEB` | attivo | `off` disattiva ricerca e lettura di pagine |
| `JARVIS_VOICE_ENGINE` | `auto` | `off` disattiva la voce; `fake` usa motori finti (test) |
| `JARVIS_STT_MODEL` | `small` | Modello faster-whisper (`tiny`, `base`, `small`, `medium`) |
| `JARVIS_TTS_VOICE_NAME` | `it_IT-paola-medium` | Voce Piper scaricata da `voice-setup` |
| `JARVIS_SEARCH_LANG` | `it` | Lingua di Wikipedia per la ricerca |
| `JARVIS_SEARXNG_URL` | — | Usa un'istanza SearXNG (anche locale) al posto di Wikipedia |
| `JARVIS_ALLOWED_HOSTS` | — | Nomi host del proxy fidato da cui i dispositivi associati raggiungono JARVIS (separati da virgola) |
| `JARVIS_WEB_ALLOW` / `JARVIS_WEB_DENY` | — | Domini consentiti o vietati, separati da virgola (`wikipedia.org,example.org`) |

Limiti noti della v0.1:
- un solo utente locale e una richiesta alla volta;
- PWA su Android/iOS e client nativo iOS per chat e conferme; connettori solo HTTP dichiarativi (niente OAuth né plugin con codice);
- con Ollama la qualità degli strumenti dipende dal modello scelto.

Accettazione e prove da fare sul tuo computer: [docs/ACCEPTANCE.md](docs/ACCEPTANCE.md). Dettagli in [docs/ROADMAP.md](docs/ROADMAP.md) e nell'ADR-0004 in [docs/DECISIONS.md](docs/DECISIONS.md).

## Da dove partire

- Per riprendere lo sviluppo da Claude Code, aprire la radice del repository e seguire [CLAUDE.md](CLAUDE.md), [ROADMAP.md](docs/ROADMAP.md) e [DEVELOPMENT.md](docs/DEVELOPMENT.md).
- Per conoscere lo scope, leggere [REQUIREMENTS.md](docs/REQUIREMENTS.md), [ARCHITECTURE.md](docs/ARCHITECTURE.md) e [TECH_STACK.md](docs/TECH_STACK.md).
- Per lavorare sui sottosistemi, consultare [AI_SYSTEM.md](docs/AI_SYSTEM.md), [MEMORY_DATA_SYNC.md](docs/MEMORY_DATA_SYNC.md), [AUTONOMY_AND_TOOLS.md](docs/AUTONOMY_AND_TOOLS.md), [SECURITY_AND_PRIVACY.md](docs/SECURITY_AND_PRIVACY.md) e [INTERFACE_AND_DEVICES.md](docs/INTERFACE_AND_DEVICES.md).
- Per validare e distribuire il prodotto, usare [DEVOPS_QA_OBSERVABILITY.md](docs/DEVOPS_QA_OBSERVABILITY.md).
- Per verificare approvazioni, lacune e proposte, consultare [DECISIONS.md](docs/DECISIONS.md).

Estrarre i 14 file insieme alla radice del monorepo privato. Il piano e il registro descrivono lo scope approvato. Il codice, i contratti versionati e le verifiche riproducibili attestano lo stato di implementazione. Questo pacchetto stabilisce il riferimento documentale e non attesta lo stato del codice.
