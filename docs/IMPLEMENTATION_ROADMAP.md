# Roadmap di implementazione

Piano incrementale dei requisiti ancora **Pianificati** o **Parziali** di [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md).
Ogni fase ha dipendenze e criteri di accettazione verificabili; nessuna fase cambia lo stack tecnologico. Le fasi già
consegnate sono nel checkpoint di [ROADMAP.md](ROADMAP.md) e nelle ADR di [DECISIONS.md](DECISIONS.md).

| Fase | Contenuto | Dipende da | Criteri di accettazione |
| --- | --- | --- | --- |
| A | **Esecuzione parallela dei piani**: passi indipendenti in parallelo con limite di concorrenza | piani (ADR-0152) | test con due passi lenti indipendenti conclusi in meno della somma; stesso rispetto di policy e conferme |
| B | **Pianificatore**: dall'obiettivo a un piano proposto dal modello, mostrato al proprietario prima dell'esecuzione | A, rotta `analysis` | il piano proposto passa la validazione, non esegue nulla senza approvazione, strumenti solo da catalogo |
| C | **Validazione dei risultati**: controlli oggettivi per passo (schema di output, condizioni di successo) e ripianificazione limitata | B | passo con output non valido fallisce con codice chiaro; al massimo una ripianificazione |
| D | **Strumenti di sviluppo**: Git (sola lettura, poi scrittura con conferma), analisi dei log, dipendenze | sandbox, descrittori | descrittori e test; ogni scrittura richiede conferma e passa dall'audit |
| E | **Strumenti di produttività**: PDF (lettura, estrazione), fogli di calcolo (CSV/XLSX in lettura), appunti | descrittori | file sintetici nei test; limiti di dimensione; nessun accesso fuori dalla workspace |
| F | **Client MCP** con elenco esplicito di server autorizzati | permessi, guardia anti-iniezione | strumenti remoti visibili nel registro con rischio dichiarato; esiti trattati come dati non attendibili |
| G | **Modelli locali** (Ollama in locale) come provider aggiuntivo e scelta in base a risorse e privacy | router | provider attivabile senza chiavi; dati sensibili solo locali; ripiego automatico verso offline |
| H | **Digital Environment Twin**: inventario unico di dispositivi, nodi, servizi e progetti con aggiornamento periodico | nodi, registro | `GET /api/twin` coerente con le fonti; nessun dato personale oltre a quelli autorizzati |
| I | **Valutazione e supervisione degli agenti**: esito per agente, limiti di risorse, risoluzione dei conflitti | registro agenti | metriche per agente nel registro; un agente oltre il limite viene fermato |
| J | **SDK e plugin nativi**: pacchetto con modello di descrittore, test di conformità, caricamento da cartella autorizzata | registro, sandbox | plugin d'esempio caricato in sandbox; permessi dichiarati e verificati |
| K | **Auto-miglioramento controllato**: proposte di modifica in un ramo isolato con test e approvazione; mai policy né privilegi | J, piani | solo proposte e test in sandbox; nessun merge né rilascio senza il proprietario |
| L | **Interfaccia**: gestione degli agenti, editor di workflow, cronologia unificata, monitor di infrastruttura | API esistenti | pagine su macOS e Windows con le stesse API; nessuna logica nel client |

Priorità consigliata: A, D, E, F (valore immediato e rischio basso), poi B–C, G, H, I, J e infine K.
