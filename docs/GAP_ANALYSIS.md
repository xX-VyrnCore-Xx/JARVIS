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
| 3.3 Importanza, consolidamento, memoria di progetto, affidabilità | **IMPLEMENTED** (ADR-0178) | Punteggio spiegabile, 4 livelli di affidabilità, progetto, consolidamento con anteprima. Mancano scadenza adattiva e riassunto di episodi | `memcog.rs` |
| 3.4 Pianificazione: grafo, dipendenze, parallelismo, ripianificazione, checkpoint, verifica (`expect`) | **IMPLEMENTED** | ADR-0152, 0154, 0160, 0161; test in `plans.rs`, `tests/planner.rs` | `plans.rs`, `orchestrator/plan_runner.rs`, `planner.rs` |
| 3.4 Priorità fra attività, pianificazione sensibile alle risorse | **MISSING** | Solo budget di passi e tempo | — |
| 3.5 Multi-agente: instradamento, delega, registro, salute, supervisione, collaborazione, contesto condiviso, ripiego, aggregazione | **IMPLEMENTED** (ADR-0177) con limiti | Parti con specialisti diversi in sequenza, contesto condiviso, ripiego sul generalista, risposta aggregata. **Non** parallela; conflitti risolti per sequenza; nessuna ripresa automatica dopo una conferma | `agents.rs`, `supervisor.rs` |

## 4. Esecuzione

| Requisito | Classe | Evidenza e divario |
| --- | --- | --- |
| Registro, scoperta, metadati, validazione input, timeout, ripetizioni, audit, cronologia | **IMPLEMENTED** | `toolsys.rs`, `tools.rs`, catalogo |
| Validazione dell'output | **PARTIAL** | `expect` nei piani e prove osservabili per le scritture (ADR-0179); non contro `output_schema` |
| Sandbox | **IMPLEMENTED** | Docker senza rete |
| Annullamento, interruzione, ripresa, persistenza, replay, checkpoint | **IMPLEMENTED** | Piani (ADR-0152); ora anche «aspetta, fermati» annulla i piani in corso (ADR-0173) |
| Pausa vera di un piano in corso | **IMPLEMENTED** | ADR-0176: sospendi/riprendi, anche da chat |
| Modifica dei vincoli durante l'esecuzione e propagazione agli agenti | **IMPLEMENTED** | ADR-0176: vincoli solo restrittivi, applicati prima di ogni passo |
| Dry run | **IMPLEMENTED** | ADR-0176: prova a secco senza effetti né decisioni nell'audit |
| Shadow mode | **MISSING** | Esecuzione parallela non vincolante per confronto: non realizzata |
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

**IMPLEMENTED** con limiti (ADR-0179): classificazione dei guasti, causa e strategie, nuovi tentativi solo dove servono, verifica con prove osservabili, diagnosi dei piani, contraddizioni nella memoria. Resta: strategia alternativa scelta e avviata da sola (oggi è una proposta da approvare) e prove per gli strumenti senza controllo semplice.

## 10. Stato del mondo

**IMPLEMENTED** (ADR-0175): rappresentazione unica con versioni, riconciliazione, dato vecchio, dipendenze e cronologia; osservato distinto da dedotto. Mancano la sincronizzazione periodica automatica, servizi del sistema operativo e dispositivi non nodi, e l'uso nella chat.

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

Nodi esecutori, dispositivi associati, notifiche, sessione condivisa del proprietario: **IMPLEMENTED**. Passaggio di un'attività fra dispositivi con ripresa unica e conflitto esplicito, livelli di autonomia per nodo: **IMPLEMENTED** (ADR-0180). Sincronizzazione offline con coda e fusione dei conflitti, livelli per i dispositivi che non sono nodi, esecuzione remota di piani: **MISSING**.

## 15. Osservabilità

Log strutturati, metriche per endpoint/strumento/agente, tracce con identificativi di correlazione, salute, arresto
ordinato, ripresa dopo il riavvio: **IMPLEMENTED** (`observability.rs`). Tracciamento distribuito completo e dashboard
diagnostica unificata: **PARTIAL**; la ricostruzione di una richiesta oggi passa da task, passi e span.

## 16. Test

Dataset per instradamento (sviluppo e validazione del proprietario nel solo corpus) e, nuovo, per i tipi di enunciato con
**insiemi separati** (`tests/fixtures/intent_dev.tsv`, `intent_validation.tsv`). Test di sicurezza (iniezione, permessi),
annullamento e ripresa, offline: esistenti. Mancano: sincronizzazione fra dispositivi, contesto con applicazione attiva,
ripristino dopo crash come test dedicato.

## Seconda analisi (dopo gli incrementi 1–8)

Requisiti ancora **MISSING** o **BLOCKED**, in ordine di valore:

1. **Apprendimento da dimostrazione** — BLOCKED: serve un registratore delle azioni sull'interfaccia (Accessibilità su macOS, UI Automation su Windows), con consenso, redazione dei segreti, validazione in sandbox e approvazione; non è verificabile in questo ambiente.
2. **Sincronizzazione offline** con coda di modifiche e fusione dei conflitti; esecuzione remota di un piano su un altro dispositivo.
3. **Shadow mode** (esecuzione non vincolante per confronto) e circuit breaker generale degli strumenti.
4. **Collaborazione parallela** fra agenti e comunicazione diretta fra agenti (oggi in sequenza con contesto condiviso).
5. **Intento appreso** (embedding o modello per i casi dubbi), contesto con applicazione/finestra attiva e schermo, apprendimento delle correzioni dell'utente sull'instradamento.
6. **Modelli locali / instradamento locale-first** — escluso su istruzione del proprietario; cache semantica, budget di token per richiesta.
7. **Interfacce grafiche** per autonomia, stato del mondo, memoria cognitiva, diagnosi e passaggi (oggi API e chat); la grafica è stata rimandata dal proprietario.
8. Sincronizzazione periodica automatica dello stato del mondo; scadenza adattiva della memoria; tracciamento distribuito completo.
