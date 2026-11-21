# JARVIS

JARVIS è un assistente personale per un singolo proprietario. Il Core coordina chat, voce, strumenti, memoria e automazioni. Puoi tenere il Core sempre attivo sul Mac e usare lo stesso archivio da browser, iPhone, Android e altri computer associati.

L’inferenza usa **Ollama Cloud** o **NVIDIA NIM**. I comandi offline, la voce e la ricerca nella memoria funzionano sul Core. Le operazioni sensibili passano da policy, conferma e audit.

## Installa JARVIS

Scarica il pacchetto dalla pagina [Releases](https://github.com/xX-VyrnCore-Xx/JARVIS/releases).

| Sistema | Pacchetto | Installazione |
|---|---|---|
| macOS Apple Silicon | `JARVIS_<versione>_aarch64.dmg` | Apri il DMG e trascina JARVIS in **Applicazioni**. |
| Windows x64 | `JARVIS_<versione>_x64-setup.exe` | Avvia il setup per l’utente corrente. |

Core e Python sono inclusi. Al primo avvio puoi installare il riconoscimento vocale, la voce italiana Paola e l’attivazione «Ehi Jarvis». Per configurare le chiavi cloud, apri **Sistema → Modelli cloud**.

### Verifica un installer

Ogni release automatica verifica l’installazione di entrambi i pacchetti. L’attestazione GitHub collega i file alla build e le firme Sigstore consentono una verifica indipendente. Usa la versione del pacchetto che hai scaricato.

```bash
gh attestation verify "./JARVIS_<versione>_aarch64.dmg" --repo xX-VyrnCore-Xx/JARVIS
```

Per verificare direttamente il file con Cosign, scarica anche il suo bundle Sigstore. Per Windows usa il nome dell’EXE nella variabile.

```bash
installer='JARVIS_<versione>_aarch64.dmg'
cosign verify-blob   --bundle "$installer.sigstore.json"   --certificate-identity-regexp '^https://github\.com/xX-VyrnCore-Xx/JARVIS/\.github/workflows/release\.yml@refs/(heads/main|tags/v[^/]+)$'   --certificate-oidc-issuer https://token.actions.githubusercontent.com   "$installer"
```

La firma che il sistema operativo riconosce dipende dalle credenziali di pubblicazione. Quando la release indica **firma nativa verificata**, macOS ha firma Developer ID e notarizzazione Apple, Windows ha Authenticode con marca temporale. Le release precedenti prive del bundle individuale si verificano con l’attestazione GitHub o con `SHA256SUMS.sigstore.json`. [Configurazione delle firme](docs/DEVELOPMENT.md#firma-degli-installer)

## Stack tecnologico

| Area | Tecnologie |
|---|---|
| Core | Rust, Axum, Tokio, Serde |
| Worker locale | Python 3, JSON Lines su stdin/stdout, JSON Schema 2020-12 |
| Desktop e Web | Tauri 2, React 19, TypeScript, Vite, PWA |
| Mobile | SwiftUI su iOS/iPadOS, Kotlin e Jetpack Compose su Android |
| Dati e memoria | SQLite con rusqlite, FTS5/BM25, vettori lessicali locali e grafo delle conoscenze |
| Inferenza | Ollama Cloud e NVIDIA NIM, routing per attività e budget, limiter NIM a 35 RPM |
| Voce | faster-whisper, Piper, openWakeWord e XTTS v2 facoltativo |
| Automazioni e sandbox | Scheduler Tokio, n8n e Python in container Docker isolati |
| Sicurezza e distribuzione | rustls, WebAuthn, portachiavi di sistema, GitHub Actions, Playwright e Sigstore |

[Stack e confini dei componenti](docs/TECH_STACK.md)

## Collega più dispositivi

1. Sul Mac, configura un proxy HTTPS privato verso `127.0.0.1:8765`, per esempio con `tailscale serve 8765`.
2. Scrivi il nome host del proxy in `remote-hosts.txt` nella cartella dati del Mac host, oppure imposta `JARVIS_ALLOWED_HOSTS`. Riavvia JARVIS per usare la porta stabile.
3. Apri **Dispositivi**, genera un invito monouso e usa il codice o il QR da un client autorizzato. Il Mac resta l’host. Le app associate vedono chat, conferme, attività e notifiche secondo i permessi assegnati.

Per un secondo desktop, imposta `JARVIS_REMOTE_URL` o salva l’origine HTTPS in `remote-core-url.txt` nella sua cartella dati. Le app iOS e Android si associano con l’origine HTTPS e il codice. [Interfacce e dispositivi](docs/INTERFACE_AND_DEVICES.md)

## Usa JARVIS

- **Chat e voce.** Premi il microfono per dettare, abilita le risposte vocali o usa «Ehi Jarvis». Con un provider cloud attivo puoi correggere la dettatura prima dell’invio. La voce è elaborata sul Core.
- **Modelli cloud.** JARVIS sceglie il modello per richieste rapide, analisi, sviluppo o contesti lunghi. L’opzione **Continua la conversazione** include fino a otto scambi testuali dello stesso provider, entro un budget stimato. In modalità **Cloud automatico** entrambi i provider configurati possono ricevere la richiesta in caso di ripiego.
- **Strumenti e connettori.** Prova «info sistema», «calcola 18 × 7» o «cerca intelligenza artificiale». In **Connettori** installi servizi dal catalogo o configuri un’API dal modulo guidato. Il Core verifica le proposte e le scritture esterne richiedono conferma.
- **Memoria.** «Ricorda che …» registra fatti e preferenze. La ricerca è locale e combina corrispondenze, sinonimi comuni e similarità. Da **Memoria** puoi correggere o eliminare un elemento.
- **Automazioni e sicurezza.** In **Automazioni** puoi pianificare comandi e attivare n8n se Docker è in esecuzione sul Core. In **Sicurezza** trovi posture, accessi e audit. In **Sistema** puoi creare e verificare backup cifrati e controllare gli aggiornamenti.

[Strumenti e automazioni](docs/AUTONOMY_AND_TOOLS.md) · [Memoria e sincronizzazione](docs/MEMORY_DATA_SYNC.md) · [Sicurezza e privacy](docs/SECURITY_AND_PRIVACY.md)

## Avvia dai sorgenti

Servono Git, Rust e Python 3.10 o successivo. La CI e gli installer usano Python 3.13.

```bash
git clone https://github.com/xX-VyrnCore-Xx/JARVIS.git
cd JARVIS
./jarvis.sh
```

Su Windows usa `jarvis.cmd` o `.\jarvis.ps1`. Le istruzioni aprono il Core su `http://localhost:8765`. Usa `jarvis.sh doctor` o `jarvis.cmd doctor` per verificare l’ambiente. `./jarvis.sh desktop` apre la finestra Tauri, che su Linux richiede WebKitGTK 4.1, GTK 3 e librsvg. Per ricompilare il frontend usa `npm ci` e `npm run build` in `apps/web`.

[Sviluppo e CI](docs/DEVELOPMENT.md) · [Architettura](docs/ARCHITECTURE.md) · [Roadmap e checkpoint](docs/ROADMAP.md)
