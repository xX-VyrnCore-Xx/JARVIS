# JARVIS — Autonomous Personal AI Ecosystem

JARVIS è un ecosistema AI privato, autonomo, modulare e multipiattaforma per un singolo proprietario. Un Core Linux coordina modelli, agenti, memoria e strumenti. I dispositivi autorizzati possono eseguire attività locali e riprendere la sincronizzazione al ritorno della connessione.

L’inferenza usa Ollama Cloud o NVIDIA NIM quando il proprietario seleziona un provider. I comandi a regole, la voce, la memoria e la persistenza restano locali. L'autonomia comprende pianificazione, esecuzione e verifica, entro i permessi del proprietario.

## Installa JARVIS

**Con il programma di installazione (consigliato):** dalla pagina Releases di GitHub scarica il file per il tuo sistema:
- **Windows x64:** `JARVIS_<versione>_x64-setup.exe`.
- **macOS:** `JARVIS_<versione>_aarch64.dmg` (chip Apple) oppure `JARVIS_<versione>_x64.dmg` (Intel).

Installalo e apri **JARVIS** dal menu delle applicazioni. Core, runtime AI e Python sono inclusi: non serve installare altro.

Al primo avvio puoi installare la voce locale con riconoscimento `small`, voce italiana Paola e «Ehi Jarvis» (circa 0,9 GB). Il download mostra l’avanzamento. Per ripetere la configurazione, apri **Sistema → Configura la voce**. Per la conversazione libera, salva una chiave in **Sistema → Modelli cloud**. Ogni release è firmata dal workflow di GitHub, senza chiavi da custodire (ADR-0020). Si verifica così:

```bash
gh attestation verify JARVIS_1.0.0_aarch64.dmg --repo xX-VyrnCore-Xx/JARVIS
```

In alternativa, con `cosign`, si verifica l'elenco firmato delle impronte e poi il file scaricato:

```bash
cosign verify-blob --bundle SHA256SUMS.sigstore.json --certificate-identity-regexp '^https://github.com/xX-VyrnCore-Xx/JARVIS/' --certificate-oidc-issuer https://token.actions.githubusercontent.com SHA256SUMS
```

```bash
sha256sum -c --ignore-missing SHA256SUMS
```

Resta pubblicata solo l'ultima versione stabile.

Senza un certificato Apple Developer o Authenticode a pagamento, i sistemi mostrano ancora un avviso al primo avvio:
- **Windows:** SmartScreen può chiedere «Ulteriori informazioni → Esegui comunque»;
- **macOS:** l'app ha una firma ad hoc: al primo avvio apri Impostazioni di Sistema → Privacy e sicurezza → «Apri comunque» (su macOS 15 il clic destro → Apri non basta più). Se compare «JARVIS è danneggiato», esegui `xattr -cr /Applications/JARVIS.app`. Dopo il primo avvio l'app toglie da sola la quarantena ai propri file (ADR-0036).

Le istruzioni qui sotto valgono per chi usa i sorgenti.

Il prototipo funziona sul tuo computer, senza cloud. Comprende chat, strumenti con permessi, conferme, memoria, workspace e audit.

**Requisiti:** Windows, macOS o Linux con [Python 3.10+](https://www.python.org/) (su Windows spunta «Add to PATH»), [Rust](https://rustup.rs/) e Git.

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

- **Comandi offline.** Il motore a regole capisce comandi come «che ore sono», «info sistema», «elenca file», «scrivi note.txt: testo», «leggi note.txt», «ricorda che …», «preferisco …», «cosa sai di …», «cosa ricordi» e «dimentica <id>».

- **Automazioni**: scrivi in chat «ogni ora info sistema», «ogni giorno alle 08:00 scrivi buongiorno.txt: ciao» o «tra 5 minuti che ore sono», oppure usa la sezione Automazioni. Le automazioni sopravvivono al riavvio e conservano la cronologia di ogni esecuzione. Si possono modificare («modifica automazione <id> ogni 2 ore» o il pulsante Modifica): ogni modifica crea una nuova versione, approvata da te.

- **Ricerca Web**: «cerca intelligenza artificiale» (Wikipedia, oppure il tuo SearXNG) e «apri https://…». Sotto le risposte che usano il Web compaiono le **fonti**, con link, ora di recupero e impronta SHA-256 della pagina letta. «Qual è la fonte?» ripropone i collegamenti associati al turno precedente. Le istruzioni contenute nelle pagine non vengono mai eseguite. Gli indirizzi locali e della rete di casa sono bloccati.
- **Voce** (tutto locale): `./jarvis.sh voice-setup` installa una volta faster-whisper, Piper e openWakeWord e scarica i modelli (circa 500 MB). Poi:
  - premi 🎤 per parlare e premilo di nuovo per inviare;
  - attiva «Risposte vocali» per farti leggere le risposte (⏹ o il microfono le interrompono);
  - attiva «Mani libere» per dire «Ehi Jarvis, …» senza toccare nulla.

  L'audio non lascia il computer e non viene salvato. Le conferme restano un clic.
- **Voce XTTS v2 facoltativa.** In una cartella modello locale metti i file originali XTTS v2 (`config.json`, `model.pth`, `vocab.json` e gli altri file del checkpoint). Usa pesi da una fonte attendibile e una registrazione WAV PCM di 3–30 secondi che hai il diritto di usare. L'app rileva `models/xtts-v2/` e `models/xtts-reference.wav` nella cartella dati, oppure accetta i percorsi `JARVIS_XTTS_DIR` e `JARVIS_XTTS_REFERENCE`. Installa `torch`, `torchaudio`, `torchcodec` e `coqui-tts==0.28.0` per il Python di JARVIS (per l’app installata usa `pip install --target <dati>/python-packages` con il Python incluso). Riavvia l’app. Piper resta disponibile come ripiego. `JARVIS_XTTS_SPEED` regola il ritmo, da 0,8 a 1,3, con valore iniziale 1,08. Con un modello cloud selezionato, la dettatura compare nella chat prima dell’invio.
- **Sviluppo in sandbox.** Avvia Docker e scarica una volta l’immagine ufficiale con `docker pull python:3.13-alpine`. In chat, «verifica sintassi Python esempio.py» analizza il file della workspace. «esegui Python in sandbox: print(2 + 2)» chiede conferma prima di eseguire il frammento. Il container riceve il sorgente su stdin, gira senza rete né file del PC montati, con limiti su tempo, memoria e processi. Il terminale dell’host richiede `JARVIS_SHELL=on` e una conferma per ogni comando.
- **Notifiche**: la campanella in alto segnala conferme in attesa, automazioni fallite e anomalie (agente AI non disponibile, audit alterato). Ogni notifica è collegata al task che l'ha generata.

- **Backup**: dalla sezione Sistema o con `./jarvis.sh backup` crei una copia cifrata dei tuoi dati, protetta da una passphrase che scegli tu. **Verifica backup** controlla la passphrase, le impronte, il database e l’audit dalla sezione Sistema senza ripristinare i dati. Per ripristinarla, con JARVIS chiuso: `./jarvis.sh restore data-backups/<file>.jbak`. Dopo un aggiornamento, `./jarvis.sh rollback` riporta i dati alla copia fatta automaticamente prima del cambio di versione.
- **Avatar interattivo.** Seleziona il nucleo nella Plancia o in Chat per parlare, interrompere una risposta o aprire la conversazione.
- **Centro sicurezza.** Controlla integrità e firme dell’audit, protezione dei segreti, dispositivi, conferme e policy Web. In chat puoi chiedere «controlla la postura di sicurezza».
- **n8n locale.** Con Docker in esecuzione, apri Automazioni → Container n8n → Abilita e Avvia. Puoi attivare un promemoria o una verifica dell’audit dal pannello o dalla chat, per esempio «n8n audit ogni 15 minuti: Verifica registro». Il container usa un volume Docker dedicato e senza rete. JARVIS mostra l’ultima esecuzione riuscita delle routine registrate quando il Core è attivo.
- **Ollama Cloud e NVIDIA NIM.** In **Sistema → Modelli cloud** salva la chiave API e l’identificativo del modello per ciascun provider. In Chat puoi scegliere il provider o, con entrambe le chiavi configurate, **Cloud automatico**. Quest’ultima opzione può inviare il messaggio a entrambi i provider e dà priorità a quello che conserva il contesto. Il provider riceve il testo del messaggio corrente e schemi pubblici di alcuni strumenti. Una sua proposta passa dalla policy del Core, incluse le conferme per le scritture. Attiva **Continua la conversazione** per includere fino a tre scambi testuali consecutivi con lo stesso provider. Memoria, risposte prodotte da strumenti e relativi risultati restano locali. NIM distanzia le richieste con un tetto di 35 RPM per Core. Entrambi rispettano `Retry-After` dopo una risposta 429. Servono accesso a Internet e una chiave abilitata al modello scelto.

- **Connettori**: nella sezione Connettori incolli il manifest JSON di un servizio esterno (esempio in `tests/contracts/fixtures/valid/connector.meteo.json`), lo autorizzi con il suo token e selezioni **Prepara** accanto a un’operazione, compili i campi e la invii alla chat. Puoi anche usare «connettore <id> <operazione> chiave=valore» in modalità offline. Il token resta cifrato nel Core e non viene mai mostrato. Le operazioni che scrivono verso l'esterno chiedono sempre conferma. «Revoca» cancella il token e blocca subito le chiamate.
- **Dal telefono**: pubblica JARVIS nella tua rete privata con un proxy HTTPS (per esempio [Tailscale](https://tailscale.com): `tailscale serve 8765`), avvialo con `JARVIS_ALLOWED_HOSTS=<nome del computer nella rete Tailscale>`, apri l'indirizzo sul telefono e inserisci il codice generato nella sezione **Dispositivi**. Puoi installarlo come app dalla schermata Home. Ogni dispositivo si revoca con un clic, con effetto immediato.
- **App iOS nativa:** apri `apps/ios/JARVIS.xcodeproj` in Xcode. Il client per iOS/iPadOS 17+ si associa allo stesso Core HTTPS della PWA e offre chat, cronologia e conferme secondo i permessi del dispositivo. Vedi [Interfacce e dispositivi](docs/INTERFACE_AND_DEVICES.md).
- **App Android**: `apps/android` (Jetpack Compose). La CI produce l'APK di debug (artefatto `jarvis-android-debug`). Si associa come il telefono, con indirizzo HTTPS e codice.
- **Altri computer (nodi esecutori)**: nella sezione Dispositivi spunta «Nodo esecutore» e genera un codice. Sull'altro computer, nella cartella di JARVIS, esegui `./jarvis.sh node <indirizzo del Core> <codice>` (Windows: `jarvis.cmd node <indirizzo> <codice>`). Poi in chat: «nodo <nome> system.info» o «nodo <nome> fs.list path=.». Le scritture chiedono conferma e la revoca ferma subito il nodo.
- **Sistema**: la sezione omonima mostra stato dei componenti, latenze misurate e lo storico delle ultime 24 ore (conservato 7 giorni), esportabile in JSON o nel formato di Prometheus. I log diagnostici sono in `data/logs/core.jsonl` e non contengono i tuoi messaggi.

Cosa osservare:
- ogni azione mostra agente, strumento, decisione di policy ed esito;
- scrivere un file o cancellare un ricordo richiede la tua conferma, valida una sola volta e per 5 minuti;
- i file restano nella cartella `data/workspace`, e ogni tentativo di uscirne viene negato;
- **Plancia** (ADR-0023): la dashboard iniziale in stile HUD olografico mostra il nucleo animato di JARVIS, le operazioni da confermare, le attività con il loro esecutore, nodi e condizioni offline, prestazioni, errori, memoria, automazioni e dispositivi, con un comando rapido. Il pulsante ◐ passa al tema chiaro;
- **Aggiornamenti** (ADR-0034): in Sistema attivi il controllo giornaliero (spento per impostazione) sul canale Stable o Beta; una nuova versione arriva come notifica con il collegamento alla release;
- **Notifiche sul telefono** (ADR-0026): in Dispositivi aggiungi ntfy o Telegram. Ricevi conferme in attesa ed errori; il testo resta sul computer, salvo diversa scelta;
- le sezioni Attività, Memoria e Audit mostrano stato, ricordi e la catena di hash verificata;
- **Memoria** (ADR-0022): fatti («ricorda che …»), preferenze («preferisco …», sempre tenute presenti) ed episodi delle attività, che scadono dopo 30 giorni (`JARVIS_EPISODIC_DAYS`, 0 li disattiva). La ricerca trova i ricordi pertinenti anche con parole flesse o senza accenti. Ogni ricordo si corregge o si cancella; il grafo delle conoscenze mostra le relazioni («mia sorella si chiama Anna»).

| Variabile | Predefinito | Effetto |
|---|---|---|
| `JARVIS_PORT` | `8765` | Porta locale (il Core ascolta solo su 127.0.0.1) |
| `JARVIS_DATA_DIR` | `./data` | Database, workspace e audit. Per ripartire da zero cancella questa cartella |
| `JARVIS_SCHEDULER_TICK_S` | `20` | Ogni quanti secondi il Core controlla le automazioni scadute |
| `JARVIS_WEB` | attivo | `off` disattiva ricerca e lettura di pagine |
| `JARVIS_VOICE_ENGINE` | `auto` | `off` disattiva la voce; `fake` usa motori finti (test) |
| `JARVIS_STT_MODEL` | `small` | Modello faster-whisper (`tiny`, `base`, `small`, `medium`) |
| `JARVIS_TTS_VOICE_NAME` | `it_IT-paola-medium` | Voce Piper scaricata da `voice-setup` |
| `JARVIS_SEARCH_LANG` | `it` | Lingua di Wikipedia per la ricerca |
| `JARVIS_SEARXNG_URL` | — | Usa un'istanza SearXNG (anche locale) al posto di Wikipedia |
| `JARVIS_ALLOWED_HOSTS` | — | Nomi host del proxy fidato da cui i dispositivi associati raggiungono JARVIS (separati da virgola) |
| `JARVIS_WEB_ALLOW` / `JARVIS_WEB_DENY` | — | Domini consentiti o vietati, separati da virgola (`wikipedia.org,example.org`) |

Limiti noti
- un solo proprietario del Core;
- PWA su Android/iOS e client nativo iOS per chat e conferme; connettori HTTP dichiarativi con token, chiave o OAuth 2.0 (niente plugin con codice);
- per proporre strumenti, il modello cloud deve supportare le chiamate a funzioni.

Accettazione e prove da fare sul tuo computer: [docs/ACCEPTANCE.md](docs/ACCEPTANCE.md). Dettagli in [docs/ROADMAP.md](docs/ROADMAP.md) e nell'ADR-0004 in [docs/DECISIONS.md](docs/DECISIONS.md).

## Da dove partire

- Per riprendere lo sviluppo da Claude Code, aprire la radice del repository e seguire [CLAUDE.md](CLAUDE.md), [ROADMAP.md](docs/ROADMAP.md) e [DEVELOPMENT.md](docs/DEVELOPMENT.md).
- Per conoscere lo scope, leggere [REQUIREMENTS.md](docs/REQUIREMENTS.md), [ARCHITECTURE.md](docs/ARCHITECTURE.md) e [TECH_STACK.md](docs/TECH_STACK.md).
- Per lavorare sui sottosistemi, consultare [AI_SYSTEM.md](docs/AI_SYSTEM.md), [MEMORY_DATA_SYNC.md](docs/MEMORY_DATA_SYNC.md), [AUTONOMY_AND_TOOLS.md](docs/AUTONOMY_AND_TOOLS.md), [SECURITY_AND_PRIVACY.md](docs/SECURITY_AND_PRIVACY.md) e [INTERFACE_AND_DEVICES.md](docs/INTERFACE_AND_DEVICES.md).
- Per validare e distribuire il prodotto, usare [DEVOPS_QA_OBSERVABILITY.md](docs/DEVOPS_QA_OBSERVABILITY.md).
- Per verificare approvazioni, lacune e proposte, consultare [DECISIONS.md](docs/DECISIONS.md).

Estrarre i 14 file insieme alla radice del monorepo privato. Il piano e il registro descrivono lo scope approvato. Il codice, i contratti versionati e le verifiche riproducibili attestano lo stato di implementazione. Questo pacchetto stabilisce il riferimento documentale e non attesta lo stato del codice.
