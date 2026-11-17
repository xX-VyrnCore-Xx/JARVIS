# Architettura JARVIS

Il Core Linux è il coordinatore principale. I nodi autorizzati dichiarano capacità e stato, applicano policy locali ed eseguono le attività assegnate. La topologia prevede LAN, accesso remoto tramite VPN mesh, comunicazione diretta e relay dove necessario. La perdita della rete lascia disponibili le capacità locali e sospende quelle che dipendono dal Core remoto o dal cloud.

```mermaid
flowchart LR
    C[Desktop / Web / Mobile] --> G[API Gateway]
    G --> R[Core Runtime]
    R --> O[Orchestrator]
    O --> A[Multi-Agent Runtime]
    A --> M[AI Gateway e Memory Engine]
    A --> P[Security Engine]
    P --> T[Motori operativi e nodi autorizzati]
    O --> B[Data & Storage]
    T --> V[Observability]
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
| Automation Engine | Trigger, esecuzioni durevoli, monitoraggio e recupero |
| Integration Engine | API, connettori, plugin, SDK e webhook |
| Security Engine | Identità, decisioni di policy, rischio e audit |
| Data & Storage Engine | Persistenza, file, sincronizzazione, backup e conflitti |
| Observability Engine | Log, metriche, trace, salute e diagnostica |

Un client autentica la richiesta. Il Core registra il task, l'Orchestrator lo scompone, gli agenti propongono le operazioni e il controllo di policy precede l'esecuzione. Ogni esecutore restituisce stato ed evidenze. L'Orchestrator verifica le condizioni di completamento e pubblica lo stato al client. Consultare [AUTONOMY_AND_TOOLS.md](AUTONOMY_AND_TOOLS.md) per esiti incerti e ripresa.

I confini applicativi usano contratti versionati. Ogni modulo espone responsabilità, API, eventi, errori, permessi e segnali di salute. Le scritture passano dal modulo proprietario. Usare HTTP per richieste, WebSocket per aggiornamenti, code persistenti per lavoro differito. RPC ed event bus specifici richiedono una scelta tecnica. Iniziare con moduli coesi e separare processi o servizi quando il confine di isolamento, prestazioni o distribuzione lo richiede. Il deployment usa systemd e Docker.

La disposizione proposta dei moduli nel repository è descritta in [DEVELOPMENT.md](DEVELOPMENT.md). Il monorepo conserva assieme codice, specifiche, contratti e verifiche.
