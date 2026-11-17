# Accettazione di JARVIS (Fasi 6 e 7)

Questo documento raccoglie ciò che serve per l'accettazione finale descritta in [DEVOPS_QA_OBSERVABILITY.md](DEVOPS_QA_OBSERVABILITY.md): matrice requisiti–verifiche, controlli automatici, prove da fare sul computer del proprietario e criteri di rilascio. Lo stato dei singoli requisiti è in [REQUIREMENTS.md](REQUIREMENTS.md#tracciabilità); qui c'è la sintesi per decidere.

## Matrice requisiti–verifiche

| ID | Verifica automatica (CI) | Prova del proprietario | Esito automatico | Limite noto |
|---|---|---|---|---|
| RF-01 Conversazione | `core/server/tests/api.rs`, E2E | P1, P2 | Verde | Qualità con modelli reali da valutare |
| RF-02 Orchestrazione | `api.rs`, contratti task | — | Verde | Un solo agente |
| RF-03 Agenti | contratti delega, `tests/ai` | — | Verde | Agente generalista unico |
| RF-04 AI Gateway | `tests/ai/test_gateway.py`, guasti in `resilience.rs` | P2 | Verde | Solo backend locali |
| RF-05 Computer Use | strumenti file e sistema, `api.rs` | P3 | Verde | File e informazioni di sistema; niente desktop o terminale |
| RF-06 Web | `web.rs` (SSRF, iniezioni), E2E | P4 | Verde | Navigazione interattiva assente |
| RF-07 Automazioni | `automation.rs`, E2E | P5 | Verde | — |
| RF-08 Memoria | `api.rs`, E2E | P1 | Verde | Nessun indice semantico |
| RF-09 Voce | `voice.rs`, `tests/ai/test_voice.py`, E2E con motori finti | P6 | Verde (finti) | Motori reali solo sul tuo computer |
| RF-10 Client | E2E Web e telefono simulato, CI desktop su 3 sistemi | P7, P8 | Verde | App native e avatar assenti |
| RF-11 Dispositivi | `devices.rs`, E2E | P8 | Verde | Nodi esecutori, mTLS e VPN mesh assenti |
| RF-12 Dati | backup, rollback | P9 | Verde | Nessuna sincronizzazione fra nodi |
| RF-13 Notifiche | `notifications.rs`, E2E | P5 | Verde | Nessun canale esterno |
| RF-14 Connettori | `connectors.rs`, E2E | P10 | Verde | Niente OAuth né plugin con codice |
| RF-15 Miglioramenti | — | — | Non iniziato | Fuori dalla prima release |
| RNF-01 Contratti | `tests/contracts`, `jarvis-contracts` | — | Verde | — |
| RNF-02 Guasti | `resilience.rs` (blocco, crash, output non valido) | — | Verde | Perdita di rete fra nodi non provata |
| RNF-03 Risorse | `tests/perf` (p95 in CI) | P11 | Verde | Misure con modelli reali da fare |
| RNF-04 Sicurezza | token, host, CSP, policy, conferme, audit, revoca | P8, P12 | Verde | Passkey, mTLS e portachiavi assenti |
| RNF-05 Local-first | nessun backend cloud | — | Verde | — |
| RNF-06 Offline | backend offline in tutta la CI | P2 | Verde | — |
| RNF-07 Osservabilità | metriche, log, esportazione | — | Verde | Trace distribuite assenti |
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
| P2 | Modello locale | Installa Ollama, `ollama pull qwen2.5:3b`, riavvia | L'intestazione mostra Ollama; la conversazione è libera; spegnendo Ollama JARVIS passa a offline e lo dichiara |
| P3 | File | «scrivi prova.txt: ciao», approva, «leggi prova.txt» | Conferma richiesta; file in `data/workspace` |
| P4 | Web reale | «cerca Rust linguaggio», poi «apri <link>» | Fonti con link, ora e SHA-256 |
| P5 | Automazioni e notifiche | «tra 2 minuti che ore sono» | Esecuzione all'ora prevista e notifica nella campanella |
| P6 | Voce | `./jarvis.sh voice-setup`, poi 🎤 e «Mani libere» | Trascrizione corretta in italiano; «Ehi Jarvis» attiva; interruzione con la voce |
| P7 | App desktop | `./jarvis.sh desktop` | Finestra propria; chiudendola si fermano Core e worker |
| P8 | Telefono | `tailscale serve 8765`, `JARVIS_ALLOWED_HOSTS=<host>`, codice dalla sezione Dispositivi | Associazione, chat dal telefono; dopo la revoca l'accesso cade subito |
| P9 | Backup e rollback | Sezione Sistema → backup; `./jarvis.sh restore <file>`; dopo un aggiornamento `./jarvis.sh rollback` | Dati ripristinati; i dati sostituiti restano in una cartella a parte |
| P10 | Connettore | Installa un connettore verso un servizio tuo, autorizza, revoca | Chiamata riuscita; dopo la revoca la chiamata è negata |
| P11 | Prestazioni | `python tests/perf/latency.py http://127.0.0.1:8765 30` con Ollama attivo | p95 entro `tests/perf/slo.json` per le serie senza modello; registrare quelle con il modello |
| P12 | Sicurezza | Dal telefono prova sezioni del proprietario; apri l'indirizzo da un host non ammesso | Accesso negato; host rifiutato |

## Criteri di rilascio

Una release `v1.0.0` si pubblica quando:
1. tutti i job delle due CI sono verdi sul commit da rilasciare;
2. le prove P1–P12 sono superate o i difetti aperti sono registrati come non bloccanti;
3. il proprietario accetta esplicitamente.

Restano decisioni del proprietario, perché richiedono suoi account, certificati o scelte:
- firma dei binari e installatori;
- canali Stable/Beta;
- VPN mesh;
- portachiavi del sistema;
- avatar e app native.

## Procedura di rilascio e rollback

**Rilascio:**
1. Aggiornare la versione in `core/server/Cargo.toml`.
2. Unire la modifica con CI verde.
3. Il proprietario crea il tag `vX.Y.Z`: il workflow di release costruisce gli archivi per i 3 sistemi con i file `.sha256` e prepara una bozza di release da approvare.

**Verifica di un archivio:**

```bash
sha256sum -c jarvis-X.Y.Z-linux-x86_64.tar.gz.sha256
```

**Rollback:**
1. Chiudere JARVIS.
2. Eseguire `./jarvis.sh rollback` (elenco delle copie: `./jarvis.sh rollback --list`).
3. Avviare la release precedente.

All'avvio di una versione diversa, il Core copia automaticamente i dati in `<dati>-rollback/` prima di aprirli e conserva le ultime 3 copie.
