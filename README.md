# JARVIS

JARVIS è un assistente personale per un singolo proprietario. Il Core coordina chat, strumenti, memoria e automazioni. Puoi tenere il Core sempre attivo sul Mac e usare lo stesso archivio dall'app desktop nativa, da Android e dagli altri computer associati.

L’inferenza usa **Ollama Cloud** o **NVIDIA NIM**. I comandi offline, la voce e la ricerca nella memoria funzionano sul Core. Le operazioni sensibili passano da policy, conferma e audit.

## Installa JARVIS

Scarica il pacchetto dalla pagina [Releases](https://github.com/xX-VyrnCore-Xx/JARVIS/releases).

| Sistema | Pacchetto | Installazione |
|---|---|---|
| macOS Apple Silicon | `JARVIS_<versione>_aarch64.dmg` | Apri il DMG e trascina JARVIS in **Applicazioni**. |
| Windows x64 | `JARVIS_<versione>_x64-setup.exe` | Avvia il setup per l’utente corrente. |
| Android | `JARVIS_<versione>_android-release.apk` | Installa l'APK release firmato (non usare APK di debug). |

Core e Python sono inclusi negli installer desktop. Il runtime vocale è disponibile nel Core, ma i controlli microfono, sintesi vocale e wake word non sono ancora esposti nella nuova app desktop nativa. L'APK Android è allegato alla stessa release; per abilitare le notifiche push configura [Firebase](docs/FIREBASE_SETUP.md).
La pagina Releases conserva una sola versione alla volta: al rilascio successivo le release e i tag precedenti vengono rimossi.
Il Core controlla ogni ora le nuove release e invia una notifica; download, verifica della firma e installazione restano manuali. Per un repository privato configura un token fine-grained in **Aggiornamenti** nell'app desktop.

### Verifica un installer

Ogni release automatica verifica gli installer desktop e la firma dell'APK Android. L’attestazione GitHub collega i file alla build e le firme Sigstore consentono una verifica indipendente. Usa la versione del pacchetto che hai scaricato.

```bash
gh attestation verify "./JARVIS_<versione>_aarch64.dmg" --repo xX-VyrnCore-Xx/JARVIS
gh attestation verify "./JARVIS_<versione>_android-release.apk" --repo xX-VyrnCore-Xx/JARVIS
```

Per verificare direttamente il file con Cosign, scarica anche il suo bundle Sigstore. Per Windows usa il nome dell’EXE nella variabile.

```bash
installer='JARVIS_<versione>_aarch64.dmg'
cosign verify-blob   --bundle "$installer.sigstore.json"   --certificate-identity-regexp '^https://github\.com/xX-VyrnCore-Xx/JARVIS/\.github/workflows/release\.yml@refs/(heads/main|tags/v[^/]+)$'   --certificate-oidc-issuer https://token.actions.githubusercontent.com   "$installer"
```

Gli installer desktop macOS e Windows non sono firmati nativamente. macOS può richiedere l’autorizzazione manuale e Windows può mostrare un avviso SmartScreen. Gli artefatti sono comunque verificabili con l’attestazione GitHub e le firme Sigstore; l’APK Android resta firmato con il keystore Android.

## Stack tecnologico

| Area | Tecnologie |
|---|---|
| Core | Rust, Axum, Tokio, Serde |
| Worker locale | Python 3, JSON Lines su stdin/stdout, JSON Schema 2020-12 |
| Desktop | Rust, egui/eframe; API HTTP del Core |
| Mobile | Kotlin e Jetpack Compose su Android |
| Dati e memoria | SQLite con rusqlite, FTS5/BM25, vettori lessicali locali e grafo delle conoscenze |
| Inferenza | Ollama Cloud e NVIDIA NIM, routing per attività e budget, limiter NIM a 35 RPM |
| Voce | faster-whisper, Piper, openWakeWord e XTTS v2 facoltativo |
| Automazioni e sandbox | Scheduler Tokio, n8n e Python in container Docker isolati |
| Sicurezza e distribuzione | rustls, WebAuthn, portachiavi di sistema, GitHub Actions, test API e Sigstore |

[Stack e confini dei componenti](docs/TECH_STACK.md)

## Collega più dispositivi

1. Sul Mac, configura un proxy HTTPS privato verso `127.0.0.1:8765`, per esempio con `tailscale serve 8765`.
2. Scrivi il nome host del proxy in `remote-hosts.txt` nella cartella dati del Mac host, oppure imposta `JARVIS_ALLOWED_HOSTS`. Riavvia JARVIS per usare la porta stabile.
3. Apri **Dispositivi**, genera un invito monouso e usa il codice o il QR da un client autorizzato. Il Mac resta l’host. Le app associate vedono chat, conferme, attività e notifiche secondo i permessi assegnati.

Per un secondo desktop, imposta `JARVIS_REMOTE_URL` o salva l'origine HTTPS in `remote-core-url.txt` nella sua cartella dati. L'app desktop e quella Android si associano con l'origine HTTPS e il codice. [Interfacce e dispositivi](docs/INTERFACE_AND_DEVICES.md)

## Usa JARVIS

- **Chat.** Invia richieste al Core e consulta la cronologia della conversazione. Le azioni sensibili restano soggette a policy e conferma.
- **Modelli cloud e voce.** La sezione Modelli AI dell'app desktop permette di configurare o rimuovere le chiavi per Ollama Cloud e NVIDIA NIM; il Core instrada le richieste al provider selezionato. Il runtime vocale è incluso nel Core, ma i controlli vocali non sono ancora disponibili nell'interfaccia desktop nativa.
- **Dispositivi e notifiche.** Associa client Android o desktop tramite invito monouso. Le notifiche push Android richiedono Firebase; il Core continua a conservarle anche quando un client non è connesso.
- **Aggiornamenti.** Il Core rileva le release GitHub ogni ora. Una repository pubblica non richiede un token; per una repository privata inserisci nella schermata **Aggiornamenti** un token fine-grained con sola lettura dei contenuti. Il token è custodito cifrato dal Core; le app notificano la release, ma installazione e aggiornamenti Android restano confermati manualmente.
- **Altre sezioni.** La navigazione nativa include attività, automazioni, memoria, competenze, workspace, connettori, dispositivi, sicurezza, audit e sistema. Alcune viste sono ancora generiche e non includono tutti i flussi guidati.

[Strumenti e automazioni](docs/AUTONOMY_AND_TOOLS.md) · [Memoria e sincronizzazione](docs/MEMORY_DATA_SYNC.md) · [Sicurezza e privacy](docs/SECURITY_AND_PRIVACY.md)

## Avvia dai sorgenti

Servono Git, Rust 1.95 o successivo e Python 3.10 o successivo. La CI e gli installer usano Python 3.13.

```bash
git clone https://github.com/xX-VyrnCore-Xx/JARVIS.git
cd JARVIS
./jarvis.sh
```

Su Windows usa `jarvis.cmd` o `.\jarvis.ps1`. `./jarvis.sh` avvia il Core API; `./jarvis.sh desktop` apre l'app nativa egui e avvia il Core in automatico. Su Linux servono le librerie X11, XCursor, XRandR, Xi, XKBCommon, OpenGL e DBus indicate in [Sviluppo e CI](docs/DEVELOPMENT.md).

[Sviluppo e CI](docs/DEVELOPMENT.md) · [Secrets GitHub Actions](docs/GITHUB_ACTIONS_SECRETS.md) · [Firebase](docs/FIREBASE_SETUP.md) · [Architettura](docs/ARCHITECTURE.md) · [Roadmap e checkpoint](docs/ROADMAP.md)
