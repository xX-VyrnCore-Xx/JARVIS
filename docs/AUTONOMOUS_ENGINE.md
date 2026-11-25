# Motore autonomo

Fonte autorevole per l'esecuzione autonoma controllata. Strumenti e policy: [TOOL_SYSTEM.md](TOOL_SYSTEM.md). Missioni,
iniziative proattive e automazioni: [AUTONOMY_AND_TOOLS.md](AUTONOMY_AND_TOOLS.md). Stato:
[IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md).

## Principi

Nessun ciclo illimitato. Ogni passo passa dalla policy, dalle regole del proprietario, dall'arresto di emergenza e
dall'audit; i contenuti esterni sono dati, mai istruzioni. L'unico modo per eseguire ciò che richiede conferma è
l'approvazione esplicita del proprietario.

## Piani (ADR-0152)

Un **piano** è un grafo di passi, ciascuno uno strumento del catalogo, con dipendenze (`depends_on`), ripetizioni
(`retries` ≤ 2), gestione dell'errore (`on_error`: `stop` o `continue`) e, facoltativamente, l'operazione inversa (`undo`).
Limiti: al massimo 20 passi, budget di tempo (predefinito 120 s, massimo 900 s) e di passi eseguiti.

```json
{"goal": "Calcola e conta", "idempotency_key": "calc-1", "run": true,
 "steps": [
  {"id": "calc",  "tool": "math.calc", "arguments": {"expression": "6*7"}},
  {"id": "words", "tool": "text.count", "depends_on": ["calc"],
   "arguments": {"text": "${calc.expression} fa ${calc.result}"}}]}
```

- **Riferimenti**: `"${passo.campo.sotto}"` intera conserva il tipo del valore; dentro un testo è sostituito dalla sua resa.
  Un riferimento a un passo non concluso o a un campo assente fa fallire il passo (`plan.bad_reference`).
- **Esecuzione**: un passo alla volta, fuori dal blocco del Core, con **checkpoint** nel database dopo ogni passo (cifrato).
  Un piano rimasto in esecuzione alla chiusura del Core diventa `interrupted` e si **riprende** con `POST /api/plans/{id}/run`.
- **Conferme (human-in-the-loop)**: un passo che richiede conferma (catalogo o regola `ask`) mette il piano in `waiting`
  senza eseguire nulla; `POST /api/plans/{id}/approve` approva i passi indicati.
- **Errori**: ripetizioni per passo; dopo il fallimento definitivo i passi dipendenti sono `skipped` e, con `on_error=stop`,
  il resto del piano è `cancelled`.
- **Annullamento**: `POST /api/plans/{id}/cancel` (ferma anche il ciclo di esecuzione).
- **Rollback**: `POST /api/plans/{id}/rollback` esegue gli `undo` dei passi riusciti, dall'ultimo al primo; quelli che
  richiedono conferma partono solo con `{"confirm": true}`.
- **Replay**: `POST /api/plans/{id}/replay` crea una copia da zero. **Idempotenza**: la stessa `idempotency_key` restituisce
  il piano già creato.
- **Cronologia**: ogni piano ha una lista di eventi (creazione, avvio, passi, ripetizioni, attese, conclusione).
- Stati: `pending`, `running`, `waiting`, `succeeded`, `failed`, `cancelled`, `budget_exceeded`, `interrupted`.

Non implementati: esecuzione **parallela** dei passi indipendenti, scomposizione automatica di un obiettivo in un piano,
ripianificazione guidata dal modello e validazione indipendente dei risultati (vedi roadmap).

## Arresto di emergenza (ADR-0151)

`POST /api/emergency {"active": true}` nega ogni strumento che modifica o esegue (profilo di sola lettura imposto), ferma i
lavori in background, interrompe i piani e sospende scheduler, missioni, automazioni e iniziative. Notifica e audit
sono sempre registrati. Si disattiva solo dal computer del proprietario.

## Agenti (ADR-0153)

`GET /api/agents` elenca gli agenti: sette specialisti (sicurezza, ricerca, sistemi, automazione, memoria, business, coding)
con competenze, strumenti concessi, rischio massimo e contesto **isolato**, più il generalista. `GET /api/agents/route?q=`
spiega perché una richiesta va a un agente (parole chiave attivate). `GET /api/models/route?q=` spiega la rotta per compito,
il modello per provider e la riservatezza (credenziali bloccate), senza inviare nulla. La delega (ADR-0040) assegna a un agente
isolato solo gli strumenti del suo ambito.
