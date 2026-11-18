# Memoria, dati e sincronizzazione

| Ambito | Responsabilità |
|---|---|
| Contesto di sessione | Turni e stato corrente del task |
| Memoria episodica | Eventi e interazioni rilevanti con provenienza |
| Memoria semantica | Conoscenze recuperabili, embeddings e knowledge graph |
| Memoria procedurale | Procedure, preferenze operative e workflow |
| Storage applicativo | Entità, file, metadati, versioni e backup |

PostgreSQL conserva lo stato centrale, SQLite la persistenza sul nodo. pgvector e Qdrant supportano la ricerca semantica. Storage documentale e object storage ospitano gli oggetti e le relative versioni. Ogni indice derivato deve poter essere ricostruito a partire dalle fonti autorizzate.

## Ciclo di vita

Classificare i dati all'ingresso e conservare provenienza, proprietario, permessi e retention. Applicare i filtri di accesso durante il recupero, includendo ricerca semantica e knowledge graph. Correzione e cancellazione devono propagarsi ai derivati e alla sincronizzazione. La retention dei backup deve descrivere quando una cancellazione diventa effettiva anche nelle copie protette.

## Offline e riconciliazione

Conservare sul dispositivo le attività differibili e le modifiche locali autorizzate. Alla riconnessione, autenticare il nodo, scambiare le revisioni, applicare le operazioni secondo il loro identificativo e confermare quelle già ricevute. Gestire cancellazioni, conflitti concorrenti e revoca di accesso. Quando la strategia del tipo di dato non consente una fusione sicura, mantenere entrambe le versioni per risoluzione. I dettagli del protocollo e le regole per tipo di dato vanno formalizzati nei contratti prima dell'implementazione.

Verificare ripetizione della stessa operazione, crash durante la sincronizzazione, divergenza di due nodi, cancellazione seguita da riconnessione tardiva e ricostruzione dell'indice. Il piano di ripristino deve coprire backup cifrati e convalida di dati, metadati e permessi dopo il restore.

## Stato del prototipo v0.1

- **Memoria del proprietario (ADR-0022):** in SQLite (`data/jarvis.sqlite3`), tabella `memory` con tipo, provenienza, data di modifica e scadenza:
  - *semantica*: fatti salvati con «ricorda che …»;
  - *procedurale*: preferenze e regole («preferisco …», «d'ora in poi …»), sempre nel contesto dell'agente;
  - *episodica*: un riassunto per ogni attività conclusa dalla chat, legato al task e con scadenza di 30 giorni (`JARVIS_EPISODIC_DAYS`; 0 disattiva). Le operazioni sulla memoria non generano episodi, così un fatto cancellato non sopravvive in un riassunto.
- **Indici derivati, ricostruibili** dalla sola tabella `memory` (`POST /api/memory/reindex` o «Ricostruisci indici»):
  - `memory_fts`: indice testuale FTS5 (BM25), senza accenti;
  - `memory_vectors`: vettore locale da 256 dimensioni (radici delle parole e trigrammi con hashing). La ricerca semantica resta offline e deterministica, senza modelli di embedding;
  - `memory_edges`: knowledge graph di triple (soggetto, relazione, oggetto) estratte dai fatti in italiano.
- **Sinonimi (ADR-0028):** con un modello di embedding di Ollama (`nomic-embed-text` o `mxbai-embed-large`, scelto nella configurazione guidata o con `JARVIS_EMBED_MODEL`) ogni elemento ha anche un vettore del modello in `memory_embeddings`, salvato con il nome del modello. La ricerca combina 30% vettore locale, 25% BM25 e fino al 60% di similarità del modello: «auto» trova «macchina». Senza Ollama, o se non risponde, si usa la ricerca locale; gli embedding mancanti si calcolano quando il servizio torna.
- **Recupero:** ricerca ibrida (60% similarità del vettore, 40% posizione BM25) con soglia minima. L'agente riceve tutte le preferenze, i fatti (tutti fino a 30, oltre solo i pertinenti) e gli episodi pertinenti alla richiesta. Lo strumento `memory.search` e i comandi «cosa sai di …» e «cerca nella memoria …» la usano dalla chat.
- **Correzione e cancellazione:** `PATCH` e `DELETE /api/memory/{id}`, con evento di audit, aggiornano subito indice, vettore e grafo. La cancellazione tramite agente richiede conferma (RF-08). I dispositivi associati non accedono alla memoria.
- **Contesto dell'agente:** gli ultimi 10 turni della conversazione e la memoria selezionata come sopra.
- **Task e conversazione:** persistono dopo un riavvio.
- **Automazioni:** le tabelle `workflows` (documento e prossima esecuzione) e `workflow_runs` (cronologia) persistono in SQLite.
- **Non ancora implementati:** PostgreSQL con pgvector o Qdrant per un Core condiviso, sincronizzazione dei dati fra nodi (i nodi esecutori non conservano dati del Core).

## Backup e ripristino (RNF-08, ADR-0012)

`./jarvis.sh backup`, oppure la sezione Sistema, crea un file `.jbak` cifrato con la passphrase del proprietario in `<dati>-backups`. Il file contiene un'istantanea coerente del database, il registro di audit, la workspace e un manifest con le impronte.

`./jarvis.sh restore <file>`, eseguito a JARVIS chiuso, verifica cifratura, impronte, catena di audit e integrità del database prima di sostituire i dati; i dati precedenti restano in `<dati>.before-restore-<istante>`.

Una cancellazione eseguita in JARVIS non modifica i backup già creati: per renderla definitiva bisogna eliminare anche i file `.jbak` precedenti.


**Database e deduplicazione (ADR-0037).** SQLite in WAL con `synchronous=NORMAL`, `busy_timeout` di 5 secondi e versione dello schema in `user_version`; manutenzione ogni 6 ore e diagnostica in `GET /api/database`. `memory.remember` non duplica un fatto già presente (confronto sul testo normalizzato).
