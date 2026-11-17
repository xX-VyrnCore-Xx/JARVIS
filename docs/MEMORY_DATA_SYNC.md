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
