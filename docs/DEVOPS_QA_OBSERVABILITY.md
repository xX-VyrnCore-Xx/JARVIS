# DevOps, QA e osservabilità

## Build e distribuzione

Il Core Linux combina servizi systemd e componenti Docker. Il progetto prevede installer multipiattaforma e aggiornamenti per modulo con verifica crittografica, canali Development, Beta e Stable, health check e rollback automatico. Ogni release registra commit, versioni dei contratti, artefatti, prerequisiti e compatibilità dei dati. Verificare in ambiente di prova upgrade, compatibilità con client già distribuiti e recupero dopo migrazione fallita.

## Verifiche

| Livello | Copertura |
|---|---|
| Unitario | Policy, routing, transizioni, conflitti e funzioni dei moduli |
| Contratti e integrazione | Compatibilità API/eventi, Rust–Python, database, provider e sincronizzazione |
| E2E | Chat, piano, azione autorizzata, voce, Web, workflow e client previsti |
| Multipiattaforma | Installazione, permessi OS, aggiornamento e compatibilità |
| Prestazioni | Latenze, risorse, carichi agentici, qualità offline e SLO |
| Sicurezza e privacy | Accesso minimo, isolamento, injection, revoca, egress e retention |
| Guasti | Fault injection, chaos engineering, riavvio, perdita di rete e provider |
| Ripristino | Restore da backup cifrato, integrità dei dati e rollback |

## Diagnosi

Correlare richiesta, task, agente, provider, strumento e nodo con identificativi condivisi. Rilevare salute dei servizi, profondità e attesa delle code, errori, latenza, risorse dei runtime, sincronizzazione e stato degli aggiornamenti. Documentare soglie e frequenza di misurazione quando i carichi di riferimento sono definiti. Minimizzare telemetria e payload come da [SECURITY_AND_PRIVACY.md](SECURITY_AND_PRIVACY.md).

## Enterprise Acceptance System

Per l'accettazione finale raccogliere la matrice requisiti–test con esiti, la compatibilità multipiattaforma, la validazione di sicurezza e privacy, le misure di prestazione rispetto agli SLO, le prove di fault tolerance, restore e rollback, la documentazione e le evidenze di deploy e aggiornamento. Registrare eventuali difetti aperti e verificare l'assenza di problemi critici o bloccanti. La chiusura richiede l'accettazione esplicita del proprietario. Valori numerici per SLO e obiettivi di recovery richiedono definizione con evidenze prima della relativa verifica.
