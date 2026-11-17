# Stack tecnologico

| Componente | Scelta | Confine |
|---|---|---|
| Core Linux | Rust | API, lifecycle, concorrenza, policy ed esecuzione |
| AI e agenti | Python | Modelli, pipeline e integrazioni AI tramite contratti con il Core |
| Desktop | Tauri, React e TypeScript | Integrazione nativa Rust e interfaccia client |
| Web | React e TypeScript | Dashboard e streaming attraverso API |
| Dati centrali | PostgreSQL | Dati applicativi, metadati e stati durevoli |
| Dati sul nodo | SQLite | Persistenza e code locali, supporto offline |
| Ricerca semantica | pgvector e Qdrant | Indice integrato e motore dedicato secondo i carichi |
| Inferenza locale | Ollama, con llama.cpp come alternativa | Adapter separati dietro AI Gateway |
| Escalation | Ollama Cloud | Accesso e payload soggetti a verifica di compatibilità e policy |
| Browser | Playwright | Navigazione e automazione Web autorizzata |
| Deploy del Core | systemd e Docker | Servizi nativi e componenti isolati |
| Versionamento | GitHub privato e mirror locale | Sorgenti, contratti e checkpoint |

Lo stack mobile richiede valutazione comparativa. Restano da selezionare le implementazioni per STT, TTS, wake word, event bus, RPC, object storage, knowledge graph, gestione dei segreti, VPN mesh e aggiornamento Desktop. Il piano conferma le relative capacità. Revisioni minime, modelli e soglie prestazionali saranno fissati con prove riproducibili.

Versionare dipendenze e contratti alla loro introduzione. Per Rust/Python, descrivere input, output, compatibilità, propagazione degli errori, timeouts e stato. Mantenere frontend e adapter indipendenti dalle credenziali dei provider. Verificare prestazioni sul primo host e documentare il profilo hardware prima di stabilire il catalogo dei modelli.

## Versioni introdotte

| Componente | Versione | Uso | Evidenza |
|---|---|---|---|
| JSON Schema | draft 2020-12 | Contratti `contracts/v1/` | ADR-0001 |
| Python | 3.13 (CI), 3.13.18 (container di sviluppo) | Test dei contratti | `.github/workflows/contracts.yml` |
| `jsonschema` (Python) | 4.26.0 | Validazione dei contratti | `tests/contracts/requirements.txt` |
| Rust | MSRV dichiarata 1.82, verificata solo con 1.97.0 | Workspace `core/` | `core/Cargo.toml` |
| `jsonschema` (Rust) | 0.30, senza feature predefinite | Validazione dei contratti nel Core | `core/Cargo.lock` |
| `sha2`, `hex`, `chrono`, `serde_json` | Vedi `core/Cargo.lock` | Catena di audit e timestamp | ADR-0002 |
| `uuid` (Rust) | 1, feature `v4` | Identificativi dei messaggi del bridge | ADR-0003 |
| Bridge Core–AI | JSON Lines su stdio (prototipo) | `core/bridge` ↔ `ai/jarvis_ai/worker.py` | ADR-0003 |
| axum / tokio | 0.8 / 1 | API HTTP locale del Core | ADR-0004 |
| rusqlite | 0.32, `bundled` | Memoria, task e conversazione | ADR-0004 |
| Ollama (opzionale) | API `/api/chat` e `/api/tags` | Backend AI locale; nessuna libreria client, solo `urllib` | ADR-0004 |
| Playwright | 1.63.0 | Test E2E dell'interfaccia | `tests/e2e/ui_smoke.cjs` |
| Interfaccia Web | HTML, CSS e JavaScript senza dipendenze | Deviazione temporanea da React e Tauri | ADR-0004 |
| Pianificatore | `tokio::time::interval` nel Core | Automazioni (RF-07) | ADR-0005 |
