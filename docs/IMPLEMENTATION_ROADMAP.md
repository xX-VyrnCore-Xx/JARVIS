# Roadmap di implementazione

Piano incrementale dei requisiti ancora **Pianificati** o **Parziali** di [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md).
Ogni fase ha dipendenze e criteri di accettazione verificabili; nessuna fase cambia lo stack tecnologico. Le fasi già
consegnate sono nel checkpoint di [ROADMAP.md](ROADMAP.md) e nelle ADR di [DECISIONS.md](DECISIONS.md).

| Fase | Contenuto | Dipende da | Criteri di accettazione |
| --- | --- | --- | --- |
| A ✔ | **Esecuzione parallela dei piani** (consegnata, ADR-0154): passi indipendenti in parallelo con limite di concorrenza | piani (ADR-0152) | test con due passi lenti indipendenti conclusi in meno della somma; stesso rispetto di policy e conferme |
| B | **Pianificatore**: dall'obiettivo a un piano proposto dal modello, mostrato al proprietario prima dell'esecuzione | A, rotta `analysis` | il piano proposto passa la validazione, non esegue nulla senza approvazione, strumenti solo da catalogo |
| C ✔ (parziale) | **Validazione dei risultati** (condizioni `expect` consegnate; ripianificazione e controllo indipendente restano): controlli oggettivi per passo (schema di output, condizioni di successo) e ripianificazione limitata | B | passo con output non valido fallisce con codice chiaro; al massimo una ripianificazione |
| D ✔ (parziale) | **Strumenti di sviluppo** (Git, log, dipendenze consegnati, ADR-0155; restano build, database, SSH): Git (sola lettura, poi scrittura con conferma), analisi dei log, dipendenze | sandbox, descrittori | descrittori e test; ogni scrittura richiede conferma e passa dall'audit |
| E ✔ (parziale) | **Strumenti di produttività** (CSV consegnato; restano PDF, XLSX, appunti): PDF (lettura, estrazione), fogli di calcolo (CSV/XLSX in lettura), appunti | descrittori | file sintetici nei test; limiti di dimensione; nessun accesso fuori dalla workspace |
| F ✔ | **Client MCP** (consegnato, ADR-0156) con elenco esplicito di server autorizzati | permessi, guardia anti-iniezione | strumenti remoti visibili nel registro con rischio dichiarato; esiti trattati come dati non attendibili |
| G | **Modelli locali** (Ollama in locale) come provider aggiuntivo e scelta in base a risorse e privacy | router | provider attivabile senza chiavi; dati sensibili solo locali; ripiego automatico verso offline |
| H ✔ | **Digital Environment Twin** (consegnato, ADR-0157): inventario unico di dispositivi, nodi, servizi e progetti con aggiornamento periodico | nodi, registro | `GET /api/twin` coerente con le fonti; nessun dato personale oltre a quelli autorizzati |
| I ✔ (parziale) | **Valutazione e supervisione degli agenti** (valutazione consegnata; restano supervisione e conflitti): esito per agente, limiti di risorse, risoluzione dei conflitti | registro agenti | metriche per agente nel registro; un agente oltre il limite viene fermato |
| J | **SDK e plugin nativi**: pacchetto con modello di descrittore, test di conformità, caricamento da cartella autorizzata | registro, sandbox | plugin d'esempio caricato in sandbox; permessi dichiarati e verificati |
| K | **Auto-miglioramento controllato**: proposte di modifica in un ramo isolato con test e approvazione; mai policy né privilegi | J, piani | solo proposte e test in sandbox; nessun merge né rilascio senza il proprietario |
| L | **Interfaccia**: gestione degli agenti, editor di workflow, cronologia unificata, monitor di infrastruttura | API esistenti | pagine su macOS e Windows con le stesse API; nessuna logica nel client |

Consegnate in questa revisione: A, F, H e le parti indicate di C, D, E, I; in più eventi di file (ADR-0158) e modelli di piano (ADR-0159). Restano, in ordine: B (pianificatore guidato dal modello), G (modelli locali), J (SDK e plugin), L (interfacce), K (auto-miglioramento controllato) e le parti rimaste di D, E, I.
