# Sistema AI e agenti

L'AI Gateway uniforma chat, streaming, tool calling, embeddings, vision ed errori. Gli adapter espongono capacità effettive e salute dei runtime. L'AI Runtime Manager carica e scarica modelli, gestisce memoria e concorrenza e applica limiti coerenti con l'hardware. La memoria applicativa resta indipendente dal modello.

## Routing

1. Classificare capacità richieste, dati, connettività e budget di risorse.
2. Selezionare un modello locale adeguato. Ollama è il runtime predefinito e llama.cpp il backend alternativo.
3. Verificare l'idoneità dell'esito. Se serve escalation, applicare prima le regole di autorizzazione e minimizzazione per Ollama Cloud.
4. Restituire esito o motivo della degradazione. Registrare runtime, latenza, fallback ed errore con metadati minimizzati.

## Agenti previsti

| Agente | Compito |
|---|---|
| General Assistant | Conversazione e richieste generali |
| Research | Ricerca, confronto e tracciabilità delle fonti |
| Coding | Sviluppo, analisi e verifica del software |
| System | Interazione con OS, processi, file e applicazioni |
| Automation | Definizione e gestione dei workflow |
| Planning | Obiettivi, priorità, dipendenze e piani |
| Memory | Recupero, aggiornamento e consolidamento |
| Vision | Analisi di immagini, schermate e contenuti visivi |
| Voice | Interazione audio e coordinamento della pipeline |
| Monitoring | Eventi, anomalie, risorse e notifiche |

Ogni delega temporanea comprende un obiettivo, un riferimento al task, contesto minimo, strumenti, limiti, timeout e condizioni di completamento. I messaggi strutturati identificano mittente, destinatario, task, payload, stato, istante e correlazione. L'Orchestrator controlla esiti ed effetti prima di chiudere il task. Le policy restano sotto il controllo del Security Engine.

Per aggiornare prompt, agenti e adapter, versionare le modifiche e provarle su casi comparabili. Verificare routing offline, rifiuto di payload non autorizzati, risposta parziale, cancellazione in streaming, agenti scaduti e fallimento di un provider.

## Stato del prototipo v0.1

L'AI Gateway (`ai/jarvis_ai/gateway.py`) espone due backend locali:
- **Ollama**, con tool calling: il catalogo degli strumenti delegati viene convertito in funzioni;
- **offline**, a regole: deterministico e sempre disponibile.

Il routing `auto` usa Ollama se risponde e ha un modello installato. Se Ollama fallisce durante una richiesta, il gateway passa al modello offline e riporta il motivo in `backend.degraded_reason`.

L'agente propone al massimo una chiamata di strumento per turno (`jarvis.agent-result/1`); il Core decide ed esegue. È attivo solo il General Assistant. L'escalation cloud non è implementata e la delega impone sempre `allow_cloud: false`.
