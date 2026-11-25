# Analisi dei divari — evoluzione cognitiva e capacità avanzate

Confronto fra i requisiti del documento «Cognitive Architecture & Advanced Capabilities Upgrade» (2026-11-25) e lo stato
reale del repository. Documenti equivalenti già presenti e **non duplicati**: audit dello stato in
[IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md), piano in [IMPLEMENTATION_ROADMAP.md](IMPLEMENTATION_ROADMAP.md),
avanzamento in [ROADMAP.md](ROADMAP.md), decisioni in [DECISIONS.md](DECISIONS.md). Qui c'è solo il confronto; l'esito
delle verifiche è in [VALIDATION_REPORT.md](VALIDATION_REPORT.md).

**Come leggere le classi.** IMPLEMENTED = completo, integrato e coperto da test eseguiti. PARTIAL = presente ma incompleto
(il limite è scritto). MISSING = assente. BROKEN = presente ma non funzionante (nessuno trovato). NEEDS_REFACTOR =
funziona ma inadeguato. BLOCKED = richiede un prerequisito esterno. **Limite del metodo:** la classe di ogni voce si basa su
documentazione dei moduli, ricerca nel codice ed esecuzione dei test; non ho esercitato ogni singola voce con un caso a
parte, quindi un IMPLEMENTED significa «c'è un test che lo copre», non «provato in ogni variante».

Aggiornato dopo l'incremento 1 (ADR-0173, motore di intento e di contesto).

## 3. Architettura cognitiva

| Requisito | Classe | Evidenza e divario | File |
| --- | --- | --- | --- |
| 3.1 Intent Engine: tipo di enunciato, multi-intento, entità, riferimenti, fiducia, ambiguità, chiarimento | **PARTIAL** (era MISSING) | Nuovo `intent.rs`: 7 tipi, multi-intento, entità, riferimenti, fiducia, domanda di chiarimento, `POST /api/intent/analyze`, indicazione al modello. Resta: classificazione per regole (non appresa), poche entità, nessun uso del modello per i casi dubbi | `intent.rs`, `agents.rs` |
| 3.1 Non dipendere da sole parole chiave | **PARTIAL** | Instradamento a due stadi (parole chiave + radici con punteggio, ADR-0171) e prompt che risolve dal contesto; nessun classificatore con embedding | `agents.rs` |
| 3.2 Context Engine | **PARTIAL** (era MISSING) | Riferimenti risolti con conversazione, piani, file citati, errori recenti, con età e `stale`. **Non c'è** applicazione/finestra attiva, repository, file selezionati né schermo nel contesto automatico (esistono come strumenti con conferma) | `intent.rs`, `orchestrator.rs` |
| 3.3 Memoria: semantica, episodica, procedurale, grafo, scadenza, provenienza, duplicati, recupero | **IMPLEMENTED** (parti) | ADR-0022/0037/0125; `memory.rs`, `memory_edges` | `memory.rs` |
| 3.3 Importanza, consolidamento, memoria di progetto/preferenze/attività, affidabilità | **PARTIAL / MISSING** | Nessun punteggio di importanza né livello di affidabilità; nessuna memoria di progetto dedicata | `memory.rs` |
| 3.4 Pianificazione: grafo, dipendenze, parallelismo, ripianificazione, checkpoint, verifica (`expect`) | **IMPLEMENTED** | ADR-0152, 0154, 0160, 0161; test in `plans.rs`, `tests/planner.rs` | `plans.rs`, `orchestrator/plan_runner.rs`, `planner.rs` |
| 3.4 Priorità fra attività, pianificazione sensibile alle risorse | **MISSING** | Solo budget di passi e tempo | — |
| 3.5 Multi-agente: instradamento, delega, registro, salute, supervisione | **PARTIAL** | Un solo ambito per richiesta; la richiesta con più ambiti va al generalista. Mancano collaborazione parallela fra specialisti, comunicazione fra agenti, risoluzione dei conflitti e aggregazione dei risultati | `agents.rs`, `supervisor.rs` |

## 4. Esecuzione

| Requisito | Classe | Evidenza e divario |
| --- | --- | --- |
| Registro, scoperta, metadati, validazione input, timeout, ripetizioni, audit, cronologia | **IMPLEMENTED** | `toolsys.rs`, `tools.rs`, catalogo |
| Validazione dell'output contro schema | **PARTIAL** | Solo `expect` nei piani |
| Sandbox | **IMPLEMENTED** | Docker senza rete |
| Annullamento, interruzione, ripresa, persistenza, replay, checkpoint | **IMPLEMENTED** | Piani (ADR-0152); ora anche «aspetta, fermati» annulla i piani in corso (ADR-0173) |
| Pausa vera di un piano in corso | **MISSING** | Esiste annulla e riprendi dopo interruzione, non «sospendi» |
| Modifica dei vincoli durante l'esecuzione e propagazione agli agenti | **MISSING** | — |
| Dry run, Shadow mode | **MISSING** | I piani si rivedono prima di avviare, ma non c'è una simulazione degli effetti |
| Circuit breaker, idempotenza | **PARTIAL** | Salute degli strumenti e blocco missioni dopo errori; nessun circuit breaker generale |
| Undo globale, rollback | **PARTIAL** | `plan_rollback` per i passi reversibili; rollback di release; nessun undo globale |

## 5–6. Apprendimento e dimostrazione

| Requisito | Classe | Evidenza e divario |
| --- | --- | --- |
| Apprendimento da feedback e correzioni | **PARTIAL** | Valutazioni dei modelli (`cloud_feedback`), correttore dei messaggi, comandi «ricorda/dimentica»; nessuna correzione che modifichi le preferenze di instradamento |
| Procedure, schemi di successo | **PARTIAL** | `improvements.rs`: salva come modello un piano riuscito più volte, con approvazione |
| Apprendimento dalle dimostrazioni | **MISSING** | Serve un registratore di azioni (finestre, tastiera) con consenso, redazione dei segreti e validazione; prerequisito: osservazione dell'interfaccia sui sistemi reali (macOS Accessibilità, Windows) non verificabile qui → **BLOCKED** per la parte di osservazione |
| Nessuna auto-modifica di codice, permessi, policy | **IMPLEMENTED** | Per costruzione (ADR-0163) |

## 7. Competenze

Registro, scoperta, metadati, attivazione, importazione/esportazione, competenze personalizzate, procedure riutilizzabili,
abilitazione/disabilitazione: **IMPLEMENTED** (ADR-0048, 0165, `skills.rs`, `plugins.rs`). Versioni con dipendenze fra
competenze e test automatici di una competenza: **PARTIAL / MISSING**. Competenze apprese: dipendono da 6.

## 8. Intelligenza proattiva

Briefing, avvisi su disco, conferme e automazioni, cose da fare, eventi sui file, iniziative: **IMPLEMENTED**
(`proactive.rs`, `watch.rs`, `missions.rs`). Rilevamento di anomalie su server, errori ricorrenti, suggerimenti di
automazione per procedure ripetitive (parziale in `improvements.rs`), scadenze di progetto, priorità e deduplicazione delle
notifiche oltre a `dedupe_key`: **PARTIAL**.

## 9. Autocorrezione

Ripianificazione con catena limitata, condizioni di successo, ripetizioni controllate: **PARTIAL**. Mancano analisi della
causa, classificazione dei guasti, strategia alternativa scelta dal sistema e rilevamento di contraddizioni.

## 10. Stato del mondo

**MISSING.** Esistono pezzi sparsi (stato di sistema, salute degli strumenti, nodi, repository via `git.*`) ma non una
rappresentazione unica con versioni, riconciliazione e distinzione fra osservato e dedotto.

## 11. Instradamento dei modelli

Instradamento per complessità e scelta del modello cloud, streaming, cache delle connessioni, fallback offline: **IMPLEMENTED**
(`cloud.rs`). Cache semantica, budget di token per richiesta oltre al contesto, privacy-aware routing esplicito
(`privacy::redact`, il testo dello schermo non va al cloud): **PARTIAL**. **Modelli locali (Ollama locale): non toccati,
per esplicita istruzione del proprietario di lasciarli fuori** (documentato; il documento attuale li cita come «local-first»:
serve una decisione per riaprirli).

## 12. Personalità e conversazione

Personalità configurabile, profilo, continuità, adattamento di tono e lunghezza, correzione dei messaggi, voce:
**IMPLEMENTED**. Frasi colloquiali: osservazioni non trattate come autorizzazioni (ADR-0173, testato), «lascia stare» e
«fermati» eseguiti. Multilingue: **PARTIAL** (italiano e inglese).

## 13. Autonomia e permessi

Policy deterministica, profili e regole che **restringono**, conferme con scadenza, audit, arresto di emergenza, segreti,
difesa dalle istruzioni nascoste (`guard.rs`): **IMPLEMENTED**. **Livelli di autonomia 0–5 configurabili per agente,
strumento, dispositivo e categoria di rischio: MISSING** (oggi: policy per rischio + profili del proprietario). È il
prossimo incremento: va progettato come ulteriore *restrizione* (mai estensione) per mantenere l'invariante di ADR-0116.

## 14. Più dispositivi

Nodi esecutori, dispositivi associati, notifiche, sessione condivisa del proprietario: **PARTIAL**. Passaggio di un'attività
fra dispositivi, sincronizzazione offline con risoluzione dei conflitti, permessi per dispositivo oltre ai permessi di
route: **MISSING**.

## 15. Osservabilità

Log strutturati, metriche per endpoint/strumento/agente, tracce con identificativi di correlazione, salute, arresto
ordinato, ripresa dopo il riavvio: **IMPLEMENTED** (`observability.rs`). Tracciamento distribuito completo e dashboard
diagnostica unificata: **PARTIAL**; la ricostruzione di una richiesta oggi passa da task, passi e span.

## 16. Test

Dataset per instradamento (sviluppo e validazione del proprietario nel solo corpus) e, nuovo, per i tipi di enunciato con
**insiemi separati** (`tests/fixtures/intent_dev.tsv`, `intent_validation.tsv`). Test di sicurezza (iniezione, permessi),
annullamento e ripresa, offline: esistenti. Mancano: sincronizzazione fra dispositivi, contesto con applicazione attiva,
ripristino dopo crash come test dedicato.

## Prossimi incrementi (ordine proposto)

1. ~~Livelli di autonomia 0–5~~ — fatto (ADR-0174).
2. **Stato del mondo** osservato vs dedotto, con versioni e rilevamento del dato vecchio, alimentato da strumenti già esistenti.
3. **Collaborazione fra specialisti**: richieste con più ambiti come piano con passi assegnati ad agenti diversi.
4. **Pausa e vincoli durante l'esecuzione**, dry run dei piani.
5. **Memoria**: importanza, affidabilità, memoria di progetto.
6. **Apprendimento da dimostrazione** (blocca: osservazione dell'interfaccia sui sistemi reali).
