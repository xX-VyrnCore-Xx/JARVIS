# Registro delle decisioni JARVIS

## Fonti

- [Pianificazione JARVIS](chatgpt-conversation://6afc1f2e-6f2c-83eb-9224-7f70b98eeb6d) contiene il piano complessivo e l'approvazione esplicita del proprietario, oltre alle risposte direttamente leggibili 81–83.
- [Pianificazione JARVIS (1)](chatgpt-conversation://6afc3d55-3e3c-83eb-b173-6a9d83fd1bd7) contiene la stesura documentale precedente. La parte leggibile è stata usata per dettagliare le sezioni di competenza.
- [Nomi repository JARVIS](chatgpt-conversation://6af62a95-e830-83eb-a359-5fb3d33c4ab1) attesta le scelte iniziali su esperienza desktop, AI ibrida, orchestrazione centrale e funzionamento offline parziale.

## Risposte numerate verificabili

| Numero | Risposta | Decisione |
|---|---|---|
| 75 | Fonte non recuperata | |
| 76 | Fonte non recuperata | |
| 77 | Fonte non recuperata | |
| 78 | Fonte non recuperata | |
| 79 | Fonte non recuperata | |
| 80 | Fonte non recuperata | |
| 81 | D | Ecosistema universale fin dall'inizio. Framework completo con catalogo iniziale prioritario ed espansione progressiva |
| 82 | D | Enterprise Acceptance System. Verifiche formali e accettazione esplicita del proprietario |
| 83 | D | Autonomous Personal AI Ecosystem. Identità definitiva del prodotto |

Per 81–83 il proprietario ha confermato l'opzione D senza precisazioni. Per 1–74 la corrispondenza tra numerazione storica e decisioni tecniche non è stata ricostruita integralmente. Le scelte del piano approvato sono riportate per argomento.

## Scelte consolidate dal piano approvato

| Argomento | Decisione | Specifica |
|---|---|---|
| Topologia | Core Linux, architettura ibrida modulare, nodi distribuiti e offline per le capacità compatibili | [Architettura](ARCHITECTURE.md) |
| AI | Orchestrator, dieci agenti, deleghe temporanee, Ollama, llama.cpp e Ollama Cloud su escalation | [Sistema AI](AI_SYSTEM.md) |
| Stack | Rust, Python, Tauri/React/TypeScript, Playwright, systemd e Docker | [Stack](TECH_STACK.md) |
| Dati | PostgreSQL, SQLite, pgvector, Qdrant, knowledge graph, storage documentale e a oggetti, sync bidirezionale | [Memoria e dati](MEMORY_DATA_SYNC.md) |
| Autonomia | Operazioni entro permessi e rischio, workflow e miglioramenti controllati | [Autonomia](AUTONOMY_AND_TOOLS.md) |
| Sicurezza | Zero Trust, passkeys e MFA, mTLS, sandbox, audit e privacy by design | [Sicurezza](SECURITY_AND_PRIVACY.md) |
| Esperienza | Desktop, Web, Mobile, voce full-duplex e avatar evolutivo | [Interfacce](INTERFACE_AND_DEVICES.md) |
| Release | Canali Stable, Beta e Development, verifica crittografica, rollback, QA e recovery | [DevOps](DEVOPS_QA_OBSERVABILITY.md) |
| Sviluppo | GitHub privato con mirror locale, Claude Code e Project Continuity Engine versionato con Git | [Sviluppo](DEVELOPMENT.md) |
| Sequenza | Contratti, prototipi, MVP avanzato, integrazione, espansione, validazione, release | [Roadmap](ROADMAP.md) |

## Scelte ancora aperte

Valutare stack mobile, motori vocali, trasporto degli eventi e RPC, implementazioni di storage e gestione segreti, soluzione VPN mesh, modalità di aggiornamento Desktop, catalogo iniziale dettagliato e valori obiettivo SLO e recovery. La struttura delle directory in DEVELOPMENT.md è una proposta operativa. I codici dei requisiti e le verifiche rappresentative sono stati aggiunti per collegare piano e implementazione.

## Aggiornamento del registro

Per un nuovo ADR registrare identificativo, data, stato, contesto, opzioni, decisione, motivazione, conseguenze, evidenza e documenti interessati. Indicare esplicitamente quando una nuova scelta sostituisce una precedente. Se si recupera una risposta storica, collegare la fonte e verificare domanda, scelta e precisazioni prima di aggiungerla.
