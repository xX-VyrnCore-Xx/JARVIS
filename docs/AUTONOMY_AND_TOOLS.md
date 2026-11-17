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
