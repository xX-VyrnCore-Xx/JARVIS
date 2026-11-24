# Sistema AI e agenti

Il Core gestisce le richieste cloud e gli strumenti. Il worker locale interpreta soltanto comandi deterministici. La voce resta sospesa in attesa di un provider audio cloud. La memoria applicativa è indipendente dai provider.

## Routing

1. Classificare capacità richieste, dati, connettività e budget di risorse.
2. Selezionare un modello di Ollama Cloud o NVIDIA NIM. Per i comandi offline il Core usa un agente deterministico a regole.
3. Verificare l’esito e mantenere il turno presso il provider iniziale anche in caso di errore.
4. Restituire l’esito o l’errore e registrare modello, latenza e stato con metadati minimizzati.

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

## Runtime, contesto e strumenti

Il worker Python usa l’agente deterministico per i comandi offline. Il Core esegue le richieste autorizzate a Ollama Cloud e NVIDIA NIM e convalida le proposte di strumenti con i contratti locali. Il routing distingue risposte rapide, sviluppo, ragionamento e contesto lungo. La complessità si misura sulla richiesta (non sulla cronologia) e le richieste ordinarie restano sul modello veloce, con ripiego dopo 40 s. I turni recenti vengono inviati al provider scelto per impostazione predefinita, senza segreti; il proprietario può spegnerli dall'interruttore «Ricorda la conversazione recente» (ADR-0130). L’inferenza può proporre soltanto calcoli isolati. I comandi che accedono a memoria, file e servizi passano dalle policy locali.

Il Core stima i token tenendo conto di sequenze di testo, punteggiatura, sintassi e Unicode. Con continuità attiva include fino a otto scambi testuali consecutivi dello stesso provider entro 16.000 token stimati; conserva il messaggio corrente e accorcia i singoli turni sui confini dei caratteri. Schemi degli strumenti, istruzioni e margine per la risposta sono inclusi nel calcolo del plafond applicativo. La memoria, i segreti rilevati e gli esiti degli strumenti restano sul Core.

## Identità, accuratezza e limiti

JARVIS adotta una voce originale: calma, competente e cordiale, professionale e concisa, con ironia discreta solo quando appropriata. Risponde nella lingua dell'utente, distingue fatti da inferenze e stime e dichiara l'incertezza quando è rilevante. Per richieste composte procede con uno strumento per volta, controlla ogni esito e continua solo se necessario. Non dichiara azioni o verifiche non confermate e spiega quando mancano capacità, dati o collegamenti, proponendo un'alternativa concreta.

Queste istruzioni orientano i modelli configurati; non modificano né addestrano i loro pesi. L'accuratezza va misurata su casi sintetici e riproducibili, separando qualità della risposta, correttezza degli strumenti e comportamento in assenza di evidenze.

## Configurazione dei provider

Il Core espone `GET /api/cloud` per lo stato e `POST /api/cloud` per configurare o rimuovere un provider. La richiesta proprietario può includere `provider` (`ollama_cloud` o `nvidia_nim`), `key` e `model`, oppure `remove_key: true`. Le chiavi sono conservate nel secret store del Core; il modello selezionato è salvato nei dati locali. La sezione desktop **Modelli AI**, disponibile solo sul Core locale, configura e rimuove le chiavi usando queste API e non le mostra dopo il salvataggio. In Modelli AI, **Testa chiave** invia un breve messaggio sintetico al provider selezionato. La verifica può consumare una richiesta e non include la conversazione. **Usa per la chat** imposta il provider per i turni automatici. Se configurato, il modello personalizzato ha priorità sui modelli automatici.

La chat cloud tenta prima di riconoscere i comandi locali. Se il parser Python non è disponibile, una richiesta generale prosegue verso il provider configurato; le richieste di fatti locali, i riferimenti alle fonti e le azioni desktop riconosciute dal Core restano locali e riportano l'errore senza inviare il testo al provider.

La chat cloud dei client desktop usa `POST /api/cloud/chat/stream` (NDJSON con frammenti `delta` ed esito `done`, ADR-0131): il testo compare mentre il modello lo genera. Il client desktop nativo usa `POST /api/chat/stream` con risposta NDJSON. Il bridge inoltra gli eventuali envelope `delegation.progress` seguiti da un esito `done` o `error`. Il worker a regole risponde in un blocco. I client che leggono un esito completo usano `POST /api/chat`.

## Voce

Il worker espone `voice.transcribe` e `voice.synthesize` sul bridge, con faster-whisper, Piper e openWakeWord in locale (ADR-0010). La trascrizione entra nella chat come testo, quindi l'agente e la policy non distinguono fra comando scritto e parlato. Lo stato dei motori fa parte della salute del worker.


**Specializzazioni (ADR-0038).** Il Core sceglie lo specialista dalla richiesta (`research`, `system`, `automation`, `memory`, `coding`) o il generalista se gli ambiti sono zero o più di uno; lo specialista riceve il proprio ruolo nel prompt e solo gli strumenti del proprio ambito più `time.now` e `memory.search`.


**Agenti temporanei (ADR-0040).** Con `agent.delegate` l'agente principale crea un agente temporaneo con un ruolo e pochi strumenti senza conferma (predefiniti di sola lettura): task figlio con la stessa traccia, al massimo 3 strumenti, nessuna sottodelega, al massimo 2 per richiesta.


**Tool calling robusto (ADR-0045).** Argomenti corretti per tipo prima dell'invio, chiamate scritte nel testo riconosciute, difetti di validazione restituiti all'agente per un nuovo tentativo. Modelli consigliati: `qwen3:4b` (meno di 16 GB di RAM) o `qwen3:8b`.


## Adattamento delle risposte

Dopo una risposta testuale riuscita, la chat macOS mostra il provider, il modello e i controlli di valutazione. Il Core conserva soltanto l’ID della risposta, il modello, il provider e la categoria scelta. Usa le ultime dodici valutazioni di quel modello presso quel provider per adattare in modo indipendente lunghezza e precisione con istruzioni predefinite. In caso di parità sulla lunghezza rimane lo stile standard. Modelli AI mostra al proprietario quanti voti usa il profilo e quali preferenze ha ricavato. Dalla cronologia macOS puoi valutare o correggere il voto di qualsiasi risposta cloud ancora valutabile. Il Core conserva un voto per risposta e riserva i metadati di valutazione allo storico del proprietario. La richiesta corrente mantiene la priorità sulle preferenze inferite. I turni recenti sono inviati al provider scelto per impostazione predefinita (ADR-0130); l'opzione «Ricorda la conversazione recente» della chat macOS e Windows li disattiva.

Quando un provider segnala un troncamento, l’orchestratore aumenta gradualmente il budget di risposta per quel modello e tipo di richiesta, entro il limite di contesto. Una risposta completa riporta gradualmente il budget al valore base. Se la risposta è vuota o troncata, l’orchestratore prova una sola volta con il modello alternativo dello stesso provider. Il Core non invia valutazioni, cronologia aggiuntiva o dataset di addestramento. L’adattamento agisce sulle istruzioni dell’app e sulla gestione delle risposte, mentre i pesi restano sotto il controllo del provider.

Il Core sospende per un minuto i modelli che il provider segnala come indisponibili. Il ripiego resta presso lo stesso provider. **Testa chiave** verifica subito il modello, anche durante la sospensione.
