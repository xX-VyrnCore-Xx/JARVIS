# Autonomia, strumenti e integrazioni

L'Orchestrator trasforma un obiettivo in un piano verificabile e aggiorna lo stato man mano che riceve gli esiti. Il perimetro comprende Computer Use, Web, produttività, sviluppo e amministrazione autorizzata, monitoraggio, workflow pianificati e integrazioni. Il Security Engine valuta ogni operazione sul dispositivo e sulla risorsa effettivi.

## Esecuzione

1. Registrare obiettivo, condizioni di successo, dipendenze, ambito e identità richiedente.
2. Convalidare gli argomenti dello strumento e valutare permessi, rischio e destinatario. Per le operazioni critiche presentare azione, destinazione ed effetto previsto prima della conferma.
3. Eseguire con risorse limitate e correlare il risultato al task. Registrare separatamente stato, effetto osservato ed eventuale errore.
4. Verificare la postcondizione, quindi completare, ripianificare o segnalare il blocco.

In caso di timeout con esito incerto, interrogare lo stato della risorsa prima di ripetere una scrittura. Usare un identificativo stabile per operazioni idempotenti e mantenere evidenze per ripresa dopo un arresto. L'annullamento interrompe il lavoro pendente e distingue gli effetti già applicati da quelli ancora in corso.

## Strumenti e workflow

Gli strumenti dichiarano input, output, ambito, prerequisiti, effetti, errori, timeout, modalità di retry e permessi. Ogni esecuzione programmata conserva istante, trigger e risultato. Un workflow conserva versione, dipendenze, operazioni e configurazione di frequenza. Applicare la policy vigente anche quando il workflow viene eseguito in background.

La decisione 81 prevede il framework di integrazione completo fin dall'inizio e un catalogo iniziale prioritario. Le categorie coprono servizi cloud e Web, produttività, sviluppo, database e infrastrutture, comunicazione, dispositivi, API, webhook e plugin. Le integrazioni concrete crescono per milestone. Il catalogo e l'ordine dei singoli connettori richiedono ancora conferma. I plugin operano con isolamento e permessi revocabili.

Le proposte di auto-evoluzione passano attraverso modifiche versionate, test isolati e i controlli di rilascio. La configurazione di sicurezza resta sotto controllo del proprietario.

## Stato del prototipo v0.1

Il catalogo iniziale è in `contracts/catalog/tools`, con un descrittore `jarvis.tool/1` per ogni strumento:

| Strumento | Rischio | Conferma del proprietario |
|---|---|---|
| `time.now`, `system.info`, `fs.list`, `fs.read`, `memory.list` | basso | no |
| `memory.remember` | medio | no |
| `fs.write`, `memory.forget` | alto | sì |

| `web.search`, `web.fetch` | medio, destinazione esterna | no (policy su domini e indirizzi) |
| `workflow.list` | basso | no |
| `workflow.run` | medio | no (esegue passi già approvati) |
| `workflow.create`, `workflow.update`, `workflow.delete` | alto | sì |

Ciclo di un'operazione: proposta dell'agente → validazione degli argomenti → decisione di policy → eventuale conferma → esecuzione nel Core → evento di audit → risposta composta dall'agente con il risultato. Un timeout o un crash del worker termina il task come `failed`, con un errore v1 a effetto `unknown`.

## Automazioni (Fase 3, RF-07)

Le automazioni (`jarvis.workflow/1`) eseguono uno o più passi del catalogo:
- a intervalli (minimo 1 minuto);
- ogni giorno a un'ora locale;
- una volta sola.

Garanzie:
- **approvazione:** il proprietario approva i passi esatti alla creazione, e l'approvazione è legata al loro digest;
- **policy:** a ogni esecuzione la policy vigente viene rivalutata. Un passo negato non viene eseguito; con `on_failure: stop` i passi successivi vengono saltati, con `continue` l'esito è `partial`;
- **evidenze:** ogni esecuzione (`jarvis.workflow-run/1`) registra trigger (`schedule`, `catch_up`, `manual`), istanti, decisione ed esito di ogni passo. Produce anche un task e gli eventi di audit;
- **riavvio:** le automazioni sopravvivono al riavvio; dopo uno spegnimento si recupera al più un'esecuzione persa.

Dettagli in ADR-0005.

**Modifica (ADR-0008):** nome, frequenza, passi e comportamento in caso di errore si modificano dalla chat («modifica automazione <id> ogni 2 ore», «modifica automazione <id>: info sistema», «rinomina automazione <id> in …») o dall'interfaccia. Ogni modifica crea una nuova versione con una nuova approvazione; le versioni precedenti e il legame fra esecuzione e versione restano consultabili.
