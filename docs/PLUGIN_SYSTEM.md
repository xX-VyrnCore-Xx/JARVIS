# Pacchetti di estensione (plugin)

> Stato: **implementato** per competenze e modelli di piano (ADR-0165). Gli strumenti nativi in codice restano **pianificati**:
> vedi «Cosa non c'è» in fondo. Fonte unica per l'SDK dichiarativo; il catalogo degli strumenti è in [TOOL_SYSTEM.md](TOOL_SYSTEM.md).

## Idea

Un pacchetto è **un file JSON** che raggruppa competenze e procedure (modelli di piano). Non contiene codice eseguibile, non usa la
rete e non porta chiavi: estende ciò che JARVIS *sa fare con gli strumenti che già ha*, non gli strumenti. Per questo si può
installare senza eseguire nulla di estraneo e ogni suo componente resta soggetto a policy, permessi, conferme, budget e audit.

## Formato (`jarvis.plugin/1`)

```json
{
  "contract": "jarvis.plugin/1",
  "name": "ambiente-dev",
  "version": "1.0.0",
  "description": "Strumenti per lo sviluppo",
  "author": "Nome",
  "skills": [
    {"name": "Controllo repository", "instructions": "Controlla lo stato del repository.",
     "triggers": ["stato repo"], "tools": ["git.status", "git.log"]}
  ],
  "plan_templates": [
    {"name": "stato", "description": "Stato del repository", "goal": "Stato di {{cartella}}",
     "params": [{"name": "cartella", "required": true}],
     "steps": [{"id": "s", "tool": "git.status", "arguments": {}}]}
  ]
}
```

- `name`: 2–30 caratteri `a-z`, `0-9`, `-`; `version`: `1.2.3`; `description` fino a 200 caratteri.
- Fino a 10 competenze (stesse regole di quelle del proprietario: strumenti del catalogo, massimo 12 per competenza) e 10 modelli
  (stessa sintassi di [AUTONOMOUS_ENGINE.md](AUTONOMOUS_ENGINE.md): `{{parametro}}`, dipendenze, condizioni `expect`, annullamenti).
- I modelli prendono il nome `<pacchetto>-<modello>` (massimo 40 caratteri) e portano il campo `plugin`.
- Il pacchetto intero pesa al massimo 64 KiB. **Qualsiasi campo non previsto è rifiutato** (ad esempio `scripts`).

## Installazione e sicurezza

| Operazione | Endpoint | Chi |
|---|---|---|
| Elenco | `GET /api/plugins` | proprietario |
| Installa o aggiorna | `POST /api/plugins` (corpo: il pacchetto) | proprietario, audit `plugin.install` con l'impronta SHA-256 |
| Rimuovi | `POST /api/plugins/remove` (`{"name": …}`) | proprietario, audit `plugin.remove` |

- **Tutto o niente.** Il pacchetto si valida per intero prima di toccare qualcosa: strumento sconosciuto (anche in `undo`), ciclo nei
  passi, parametri mancanti, nome non valido o duplicato → nessuna modifica. Un aggiornamento non valido lascia la versione installata.
- **Aggiornare** lo stesso nome sostituisce competenze e modelli senza duplicarli; **rimuovere** elimina esattamente ciò che il
  pacchetto aveva portato.
- **Niente privilegi in più.** Un pacchetto non può concedersi strumenti non presenti nel catalogo, togliere conferme (`git.commit`,
  `clipboard.*`, `mcp.call` restano da confermare), allargare permessi o aggirare l'arresto di emergenza. I dispositivi associati non
  possono installare pacchetti.
- Limiti: 20 pacchetti; i modelli personali (compresi quelli dei pacchetti) sono al massimo 30; il nome di un modello non può
  coincidere con uno incorporato.

## Scrivere uno strumento nuovo (autori del Core)

Per aggiungere uno **strumento** serve codice nel Core: la procedura è in [TOOL_SYSTEM.md](TOOL_SYSTEM.md#sdk-aggiungere-uno-strumento).
Un pacchetto può poi usarlo in competenze e modelli, come qualsiasi altro.

## Cosa non c'è (pianificato)

- Strumenti nativi caricati da una cartella: richiederebbero un'esecuzione isolata (sandbox) del codice del plugin e una firma
  degli autori; finché non esistono, **non si carica codice di terzi**.
- Distribuzione e firma dei pacchetti, interfaccia grafica di gestione (oggi via API).
