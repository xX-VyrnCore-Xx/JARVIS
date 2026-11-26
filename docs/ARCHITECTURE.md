# Architettura JARVIS

Il Core Linux è il coordinatore principale. I nodi autorizzati dichiarano capacità e stato, applicano policy locali ed eseguono le attività assegnate. La topologia prevede LAN, accesso remoto tramite VPN mesh, comunicazione diretta e relay dove necessario. La perdita della rete lascia disponibili le capacità locali e sospende quelle che dipendono dal Core remoto o dal cloud.

```mermaid
flowchart LR
    C[Desktop / Android] --> G[API Gateway]
    G --> R[Core Runtime]
    R --> O[Orchestrator]
    O --> A[Multi-Agent Runtime]
    A --> M[AI Gateway e Memory Engine]
    A --> P[Security Engine]
    P --> T[Motori operativi e nodi autorizzati]
    O --> B[Data & Storage]
    T --> V[Observability]
```

### Mappa del repository

Il diagramma segue i moduli realmente presenti nel codice (`core/server/src`, `ai/jarvis_ai`, `apps`, `contracts`, `.github`): le frecce indicano chi chiama chi e **ogni modulo del Core compare almeno una volta**. Un test (`tests/contracts/test_architecture_diagram.py`) lo verifica a ogni modifica, così il diagramma non resta indietro rispetto al codice. Si può usare anche come ingresso per strumenti di visualizzazione dei repository.

```mermaid
%%{init: {"theme": "base", "themeVariables": {"fontFamily": "Inter, Segoe UI, sans-serif", "fontSize": "14px", "lineColor": "#64748b"}}}%%
flowchart TB
    subgraph Client["Client (apps/)"]
        direction LR
        MAC["macOS SwiftUI<br/>Plancia, chat, avatar, Voce, Mappa, Privacy, Ricerche"]
        EGUI["Desktop egui<br/>macOS e Windows"]
        AND["Android<br/>Kotlin, FCM"]
        WEB["Web"]
    end

    subgraph Core["Core Rust (core/server/src)"]
        direction TB
        API["api, openapi, tls, config, dashboard<br/>HTTP, WebSocket, SSE"]

        subgraph Trust["Identità e protezione"]
            SEC["accounts, passkeys, secrets, chat_crypto, permissions, permission_history,<br/>policy, guard, audit, privacy, rollback"]
        end

        ORCH["orchestrator<br/>task, passi, deleghe, piani, missioni"]

        subgraph Brain["Cognizione"]
            INT["intent, dialogue, followup, dispatch<br/>atti di conversazione, vincoli, squadra di agenti"]
            AGT["agents, expert, skills, prompts, promptfix, collab"]
            PLAN["planner, plans, missions, proactive, improvements, reflect"]
            MEM["memory, memcog, world, handoff, persona, profile, research"]
        end

        subgraph Tools["Strumenti"]
            TSYS["tools, toolsys, toolbox, catalog, datatools, executive"]
            OPS["appcontrol, screen, desktop, docs, devtools, codemap, assets, workspaces, speech"]
            NET["web, browser, net, maps, connectors, connector_catalog, oauth, mcp, mcp_client, plugins, channels"]
        end

        subgraph Run["Esecuzione e risorse"]
            SB["sandbox, container_runtime, n8n"]
            SUP["supervisor, perf, watch, automation, setup, backup"]
            NODE["nodes, node_agent, tailscale, push, updates, selfupdate"]
        end

        CLOUD["cloud<br/>NVIDIA NIM, continuazione e retry"]
        STORE[("store<br/>SQLite, file dati, indici")]
        OBS["observability, sysstats"]
    end

    subgraph AI["Runtime AI Python (ai/jarvis_ai)"]
        GW["gateway.py<br/>routing e tool calling"]
        VOICE["voice.py, visemes.py<br/>STT, TTS, wake word"]
        WK["worker.py, contracts.py<br/>envelope JSON Lines"]
    end

    subgraph Contracts["contracts/"]
        CAT["catalog/tools e connectors<br/>contratti"]
        SCH["v1/*.schema.json<br/>envelope e API"]
    end

    subgraph Delivery["Consegna (.github, scripts, tests)"]
        CI["contracts.yml<br/>fmt, clippy, test, e2e, avvio reale"]
        REL["release.yml, release-after-ci.yml<br/>installer macOS e Windows, APK"]
        TST["tests/<br/>contracts, ai, e2e, data (dataset)"]
    end

    EXT["Servizi esterni<br/>NVIDIA NIM, server MCP, Gmail, Calendar, GitHub, Hugging Face, Tailscale"]

    MAC --> API
    EGUI --> API
    AND --> API
    WEB --> API
    API --> SEC --> ORCH
    ORCH --> INT --> AGT
    INT --> PLAN
    ORCH --> PLAN
    ORCH --> MEM
    ORCH --> TSYS --> OPS
    TSYS --> NET
    TSYS --> SB
    ORCH --> WK --> GW
    GW --> VOICE
    ORCH --> CLOUD
    NET --> EXT
    CLOUD --> EXT
    NODE --> EXT
    SUP --> ORCH
    NODE --> API
    ORCH --> STORE
    MEM --> STORE
    ORCH --> OBS
    CAT -. valida .-> TSYS
    SCH -. valida .-> WK
    TST -. verifica .-> Core
    CI --> TST
    CI --> REL
    REL -. pubblica .-> NODE

    classDef client fill:#e0f2fe,stroke:#0284c7,color:#0c4a6e
    classDef core fill:#ede9fe,stroke:#7c3aed,color:#3b0764
    classDef trust fill:#fee2e2,stroke:#dc2626,color:#7f1d1d
    classDef brain fill:#fef3c7,stroke:#d97706,color:#78350f
    classDef tools fill:#dcfce7,stroke:#16a34a,color:#14532d
    classDef run fill:#f1f5f9,stroke:#475569,color:#0f172a
    classDef data fill:#fce7f3,stroke:#db2777,color:#831843
    classDef ai fill:#cffafe,stroke:#0891b2,color:#164e63
    classDef ext fill:#ffffff,stroke:#94a3b8,color:#334155,stroke-dasharray: 4 3
    class MAC,EGUI,AND,WEB client
    class API,ORCH core
    class SEC trust
    class INT,AGT,PLAN,MEM brain
    class TSYS,OPS,NET tools
    class SB,SUP,NODE,OBS run
    class STORE,CLOUD data
    class GW,VOICE,WK ai
    class EXT ext
```

#### Da una frase a un risultato (cognizione)

Come una frase del proprietario diventa lavoro, senza mai concedere permessi per il solo fatto di averla capita:

```mermaid
%%{init: {"theme": "base", "themeVariables": {"fontFamily": "Inter, Segoe UI, sans-serif", "fontSize": "14px"}}}%%
flowchart LR
    U["Frase<br/>«a che punto siamo?»<br/>«sistema solo il secondo»"] --> I["intent<br/>tipo, entità, riferimenti"]
    I --> D["dialogue<br/>atto di conversazione"]
    I --> F["followup<br/>seguito e vincoli"]
    I --> S["dispatch<br/>agente principale e supporto,<br/>archetipo composito"]
    D -- presenza, stato, apertura --> R["Risposta del Core<br/>solo dati reali"]
    D -- stile e metodo --> H["Indicazioni al modello"]
    F -- indicazioni --> H
    S -- squadra --> H
    F -- "vincoli (non toccare, non pubblicare)" --> B["Blocco del Core<br/>a ogni chiamata di strumento"]
    H --> M["Modello e agenti"]
    M --> P["policy, conferme, audit"]
    B --> P
    P --> T["Strumenti"]
    classDef gate fill:#fee2e2,stroke:#dc2626,color:#7f1d1d
    classDef mind fill:#fef3c7,stroke:#d97706,color:#78350f
    class B,P gate
    class I,D,F,S mind
```

#### Consegna e aggiornamento

```mermaid
%%{init: {"theme": "base", "themeVariables": {"fontFamily": "Inter, Segoe UI, sans-serif", "fontSize": "14px"}}}%%
flowchart LR
    PR["Pull request"] --> CI["CI<br/>fmt, clippy, test, contratti,<br/>e2e, audit dipendenze"]
    CI --> RUN["Avvio reale<br/>Linux, macOS, Windows"]
    CI --> APP["App desktop<br/>Swift e egui"]
    RUN --> MAIN["main"]
    APP --> MAIN
    MAIN --> REL["Release<br/>installer, APK, SHA256SUMS"]
    REL --> UPD["updates e selfupdate<br/>controllo, download verificato"]
    UPD --> SCRIPT["Script di installazione<br/>attesa, sostituzione, riavvio sempre,<br/>apply.log"]
    SCRIPT --> NEW["Nuova versione<br/>esito letto al riavvio"]
```

### Ciclo di una richiesta

```mermaid
sequenceDiagram
    participant U as Client
    participant A as API e sicurezza
    participant O as Orchestrator
    participant M as Modello cloud
    participant T as Strumento
    U->>A: richiesta autenticata
    A->>O: task registrato
    O->>M: contesto, strumenti consentiti
    M-->>O: testo o proposta di strumento
    O->>A: verifica permessi, autonomia, rischio
    alt serve conferma
        A-->>U: conferma richiesta (avatar ambra)
        U->>A: approva o rifiuta
    end
    A->>T: esecuzione confinata
    T-->>O: esito ed evidenze
    O-->>U: risposta in streaming, audit registrato
```

| Modulo | Responsabilità |
|---|---|
| Core Runtime | Lifecycle, configurazione, health, risorse, code e scheduling |
| Orchestrator | Piani, dipendenze, delega, stato, annullamento e verifica |
| Multi-Agent Runtime | Identità, contesto e strumenti degli agenti, lifecycle delle deleghe |
| AI Gateway | Contratto comune per inferenza, streaming, tool calling e routing |
| Memory Engine | Memoria autorizzata, recupero, consolidamento e correzione |
| Voice Engine | Acquisizione, wake word, STT, TTS e stream vocale |
| Computer Use Engine | Operazioni OS, file, terminale e interfacce di un nodo |
| Web Agent Engine | Ricerca, navigazione, interazione e verifica fonti |
| Voice Engine | STT, TTS e parola di attivazione locali nel worker; acquisizione e riproduzione nell'interfaccia | `ai/jarvis_ai/voice.py`, `core/server/src/api.rs` (`/api/voice/*`) |
| Client desktop | App egui che avvia e supervisiona il Core, o si associa a un Core remoto | `apps/desktop` |
| Automation Engine | Trigger, esecuzioni durevoli, monitoraggio e recupero |
| Integration Engine | API, connettori, plugin, SDK e webhook |
| Security Engine | Identità, decisioni di policy, rischio e audit |
| Data & Storage Engine | Persistenza, file, sincronizzazione, backup e conflitti |
| Observability Engine | Log, metriche, trace, salute e diagnostica |

Un client autentica la richiesta. Il Core registra il task, l'Orchestrator lo scompone, gli agenti propongono le operazioni e il controllo di policy precede l'esecuzione. Ogni esecutore restituisce stato ed evidenze. L'Orchestrator verifica le condizioni di completamento e pubblica lo stato al client. Consultare [AUTONOMY_AND_TOOLS.md](AUTONOMY_AND_TOOLS.md) per esiti incerti e ripresa.

I confini applicativi usano contratti versionati. Ogni modulo espone responsabilità, API, eventi, errori, permessi e segnali di salute. Le scritture passano dal modulo proprietario. Usare HTTP per richieste, WebSocket per aggiornamenti, code persistenti per lavoro differito. RPC ed event bus specifici richiedono una scelta tecnica. Nel frattempo il prototipo di confine Core–AI usa JSON Lines su stdio con `jarvis.envelope/1` (ADR-0003, `core/bridge`). Iniziare con moduli coesi e separare processi o servizi quando il confine di isolamento, prestazioni o distribuzione lo richiede. Il deployment usa systemd e Docker.

La disposizione proposta dei moduli nel repository è descritta in [DEVELOPMENT.md](DEVELOPMENT.md). Il monorepo conserva assieme codice, specifiche, contratti e verifiche.

## Stato del prototipo v0.1

| Modulo previsto | Implementazione attuale | Codice |
|---|---|---|
| API Gateway e client nativi | API HTTP locale per app desktop e Android | `core/server/src/api.rs`, `apps/desktop`, `apps/android` |
| Core Runtime e Orchestrator | Task `jarvis.task/1`, delega, ripresa dopo conferma, una richiesta alla volta | `core/server/src/orchestrator.rs` |
| Multi-Agent Runtime | Un agente (General Assistant) in un worker Python supervisionato e riavviato se termina | `core/bridge`, `ai/jarvis_ai/worker.py` |
| AI Gateway | Ollama locale e modello offline, con degradazione dichiarata | `ai/jarvis_ai/gateway.py` |
| Security Engine | Policy a regole, conferme monouso, audit a catena di hash | `core/server/src/policy.rs`, `audit.rs` |
| Computer Use (parziale) | File nella workspace e informazioni di sistema | `core/server/src/tools.rs` |
| Memory e Data & Storage | Memoria, task e conversazione in SQLite | `core/server/src/store.rs` |

| Automation Engine | Workflow pianificati, pianificatore nel Core, cronologia, approvazione legata ai passi | `core/server/src/automation.rs`, `orchestrator.rs` |
| Observability (parziale) | Notifiche correlate, monitor delle anomalie, metriche di latenza ed errori, log strutturati correlati | `observability.rs`, `orchestrator.rs`, `/api/metrics` |
| Web Agent (parziale) | Ricerca (Wikipedia o SearXNG) e lettura di pagine con provenienza, difese SSRF e iniezione | `core/server/src/web.rs` |

Il client Web/PWA non è distribuito; il Core serve solo API HTTP ai client nativi. La UI egui non ha ancora parità con tutte le schermate e le funzioni vocali precedenti. Non ancora implementati: navigazione interattiva con Playwright, Integration Engine, nodi distribuiti, sincronizzazione, metriche e trace.
