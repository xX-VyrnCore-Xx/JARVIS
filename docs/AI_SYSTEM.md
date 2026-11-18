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

Il routing `auto` usa Ollama se risponde e ha un modello installato. Se Ollama fallisce durante una richiesta, il gateway passa al modello offline e riporta il motivo in `backend.degraded_reason`. Dopo 15 secondi dalla caduta o da una verifica fallita, la richiesta successiva controlla Ollama e ripristina il backend se il modello è disponibile. Anche un modello installato dopo la partenza viene rilevato con lo stesso meccanismo.

L'agente propone al massimo una chiamata di strumento per turno (`jarvis.agent-result/1`); il Core decide ed esegue. È attivo solo il General Assistant. L'escalation cloud non è implementata e la delega impone sempre `allow_cloud: false`.

Con gli strumenti Web il prompt di sistema chiede di citare le fonti con [n] e l'URL e di non seguire istruzioni contenute nelle pagine. I risultati Web arrivano al modello racchiusi tra marcatori di contenuto non attendibile. Il Core allega comunque le fonti alla risposta (ADR-0007).

## Budget dei token (ADR-0024)

Ogni turno verso il modello locale ha un costo fisso e uno variabile. Il Core e il worker li contengono così:
- **Cronologia:** al massimo 10 turni entro `JARVIS_CONTEXT_CHARS` (9000 caratteri, circa 2.500 token). L'ultimo messaggio resta intero; i turni precedenti sono accorciati a 700 caratteri e i più vecchi esclusi quando il budget finisce.
- **Memoria:** prima le preferenze, poi i ricordi pertinenti, fino a 3000 caratteri.
- **Strumenti:** il modello riceve il nucleo (`time.now`, `system.info`, `memory.remember`, `memory.search`) più i gruppi pertinenti alla richiesta: file, Web, automazioni, memoria, connettori e nodi citati. Al massimo 10. `JARVIS_TOOL_SELECTION=all` li invia tutti.
- **Schemi compatti:** al modello arrivano gli schemi senza i vincoli di lunghezza, che il Core verifica comunque con i contratti. L'esito dello strumento è serializzato in JSON compatto.
- **Ollama:** finestra `num_ctx` esplicita (`JARVIS_OLLAMA_NUM_CTX`, 8192) e modello tenuto in memoria per 10 minuti (`keep_alive`), così le richieste ravvicinate non lo ricaricano.
- **Misura:** l'esito dell'agente riporta `usage` (token in ingresso, generati, strumenti inviati). Il Core li somma nei contatori `ai.tokens.prompt`, `ai.tokens.output` e `ai.tools.sent`, visibili in Plancia, Sistema e nell'esportazione Prometheus.

## Voce

Il worker espone `voice.transcribe` e `voice.synthesize` sul bridge, con faster-whisper, Piper e openWakeWord in locale (ADR-0010). La trascrizione entra nella chat come testo, quindi l'agente e la policy non distinguono fra comando scritto e parlato. Lo stato dei motori fa parte della salute del worker.
