# Stack tecnologico di JARVIS

| Componente | Tecnologie in uso | Responsabilità |
|---|---|---|
| Core multipiattaforma | Rust, Axum 0.8, Tokio 1, Serde | API HTTP, orchestrazione, policy, audit, scheduler e strumenti |
| Bridge e contratti | JSON Lines su stdin/stdout, JSON Schema 2020-12 | Deleghe, esiti, progressi ed errori fra Core e worker Python |
| Desktop | Rust, egui/eframe 0.37, X11/OpenGL su Linux | Chat, viste API del Core, associazione remota e notifiche native |
| App mobile | Kotlin e Jetpack Compose su Android | Chat, attività e notifiche del Core HTTPS; push FCM in background con consenso |
| Dati | SQLite con rusqlite 0.43, FTS5/BM25 | Conversazioni, attività, notifiche, memoria e automazioni |
| Memoria | Vettori lessicali locali, FTS5 e grafo in SQLite | Recupero di fatti, preferenze ed episodi |
| Inferenza | Ollama Cloud e NVIDIA NIM via HTTPS | Routing per attività e contesto, limitazione delle richieste, proposte di strumenti |
| Worker locale | Python 3, faster-whisper, Piper, openWakeWord, XTTS v2 facoltativo | Comandi deterministici, riconoscimento, sintesi e attivazione vocale |
| Automazioni | Tokio e n8n isolato tramite Docker | Routine persistenti e sandbox Python con risorse limitate |
| Sicurezza | rustls, WebAuthn, portachiavi di sistema, Argon2 e ChaCha20-Poly1305 | Identità, passkey, protezione dei segreti e backup cifrati |
| Qualità e distribuzione | GitHub Actions, test API, NSIS, Sigstore | Test multipiattaforma, pacchetti DMG e EXE, attestazione e firma |

Le versioni esatte sono risolte in `core/Cargo.lock`, `apps/desktop/src-tauri/Cargo.lock` e `ai/requirements-voice.txt`. Il lock Python della voce blocca tutte le dipendenze transitive con hash SHA-256. Il Core espone API, non distribuisce un client Web. [Architettura](ARCHITECTURE.md) · [Sviluppo](DEVELOPMENT.md)
