# JARVIS — Autonomous Personal AI Ecosystem

JARVIS è un ecosistema AI privato, autonomo, modulare e multipiattaforma per un singolo proprietario. Un Core Linux coordina modelli, agenti, memoria e strumenti. I dispositivi autorizzati possono eseguire attività locali e riprendere la sincronizzazione al ritorno della connessione.

L'inferenza e la persistenza privilegiano l'ambiente locale. Il cloud interviene quando serve una capacità superiore e la policy autorizza la trasmissione. L'autonomia comprende pianificazione, esecuzione e verifica, entro i permessi del proprietario.

## Prova il prototipo (v0.1)

Il prototipo funziona sul tuo computer, senza cloud. Comprende chat, strumenti con permessi, conferme, memoria, workspace e audit.

**Requisiti:** Linux o macOS (su Windows usa WSL2), [Python 3.10+](https://www.python.org/), [Rust](https://rustup.rs/) e Git. Opzionale: [Ollama](https://ollama.com) per la conversazione libera con un modello locale.

```bash
git clone https://github.com/xX-VyrnCore-Xx/JARVIS.git
```

```bash
cd JARVIS && ./jarvis.sh
```

Poi apri **http://localhost:8765**. La prima compilazione richiede qualche minuto.

- **Senza Ollama** JARVIS parte in modalità *offline*: un modello a regole capisce comandi come «che ore sono», «info sistema», «elenca file», «scrivi note.txt: testo», «leggi note.txt», «ricorda che …», «cosa ricordi» e «dimentica <id>».
- **Con Ollama** installa un modello con supporto agli strumenti (per esempio `ollama pull qwen2.5:3b`) e riavvia `./jarvis.sh`: JARVIS lo rileva da solo.

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

Limiti noti della v0.1:
- un solo utente locale e una richiesta alla volta;
- nessuna voce, nessun client desktop o mobile, nessun Web Agent;
- con Ollama la qualità degli strumenti dipende dal modello scelto.

Dettagli in [docs/ROADMAP.md](docs/ROADMAP.md) e nell'ADR-0004 in [docs/DECISIONS.md](docs/DECISIONS.md).

## Da dove partire

- Per riprendere lo sviluppo da Claude Code, aprire la radice del repository e seguire [CLAUDE.md](CLAUDE.md), [ROADMAP.md](docs/ROADMAP.md) e [DEVELOPMENT.md](docs/DEVELOPMENT.md).
- Per conoscere lo scope, leggere [REQUIREMENTS.md](docs/REQUIREMENTS.md), [ARCHITECTURE.md](docs/ARCHITECTURE.md) e [TECH_STACK.md](docs/TECH_STACK.md).
- Per lavorare sui sottosistemi, consultare [AI_SYSTEM.md](docs/AI_SYSTEM.md), [MEMORY_DATA_SYNC.md](docs/MEMORY_DATA_SYNC.md), [AUTONOMY_AND_TOOLS.md](docs/AUTONOMY_AND_TOOLS.md), [SECURITY_AND_PRIVACY.md](docs/SECURITY_AND_PRIVACY.md) e [INTERFACE_AND_DEVICES.md](docs/INTERFACE_AND_DEVICES.md).
- Per validare e distribuire il prodotto, usare [DEVOPS_QA_OBSERVABILITY.md](docs/DEVOPS_QA_OBSERVABILITY.md).
- Per verificare approvazioni, lacune e proposte, consultare [DECISIONS.md](docs/DECISIONS.md).

Estrarre i 14 file insieme alla radice del monorepo privato. Il piano e il registro descrivono lo scope approvato. Il codice, i contratti versionati e le verifiche riproducibili attestano lo stato di implementazione. Questo pacchetto stabilisce il riferimento documentale e non attesta lo stato del codice.
