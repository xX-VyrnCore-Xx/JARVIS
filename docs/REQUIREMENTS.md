# Requisiti JARVIS

Gli identificativi qui assegnati servono per la tracciabilità dei test. La sequenza di rilascio è in [ROADMAP.md](ROADMAP.md).

| ID | Capacità richiesta | Verifica rappresentativa |
|---|---|---|
| RF-01 | Chat testuale e vocale con contesto autorizzato | Una conversazione riprende con il contesto pertinente |
| RF-02 | Pianificare obiettivi, dipendenze e task | Un piano produce risultati verificati e rende visibili gli errori |
| RF-03 | Coordinare dieci specializzazioni e agenti temporanei | Un task delegato rispetta scopo, strumenti e risorse concessi |
| RF-04 | Usare modelli locali e cloud mediante AI Gateway | Un errore del provider preserva lo stato e il routing rispetta la policy |
| RF-05 | Controllare file, processi, desktop e terminale | Un'azione autorizzata produce un effetto osservabile sul nodo selezionato |
| RF-06 | Ricercare, navigare e verificare fonti | Una ricerca riporta fonti e provenienza delle evidenze |
| RF-07 | Creare, pianificare e monitorare workflow | Un workflow sopravvive al riavvio e conserva la cronologia delle esecuzioni |
| RF-08 | Recuperare e aggiornare memoria persistente | L'utente può correggere o cancellare memoria e derivati |
| RF-09 | Wake word, STT/TTS in streaming, full-duplex e interruzione vocale | L'utente interrompe la risposta e vede lo stato del microfono |
| RF-10 | Desktop Windows, macOS e Linux, Web, Mobile Android/iOS e avatar evolutivo | I client previsti dalla milestone superano i rispettivi flussi E2E |
| RF-11 | Identificare e coordinare nodi autorizzati | Un dispositivo revocato perde accesso e assegnazioni |
| RF-12 | Versionare, indicizzare, sincronizzare e proteggere dati e file | Una modifica concorrente preserva le versioni fino alla risoluzione |
| RF-13 | Monitorare risorse, anomalie, attività e notifiche | Un guasto simulato è rilevabile e correlabile al task |
| RF-14 | Plugin, API, webhook, connettori e SDK | Un connettore può essere installato, autorizzato e revocato |
| RF-15 | Proporre miglioramenti controllati a codice e componenti | Una modifica passa per versionamento, prove e controlli previsti |

Lo scope operativo include produttività e documenti, email e calendario, sviluppo software con Git, build e deployment, interazione amministrativa autorizzata con servizi e database, analisi di immagini, audio, video e schermate. Ogni capacità dipende dal supporto del nodo e dai permessi per il task.

| ID | Vincolo | Evidenza |
|---|---|---|
| RNF-01 | Moduli sostituibili con contratti versionati | Prove di compatibilità tra client e servizi |
| RNF-02 | Guasti isolati e degradazione controllata | Fault injection con esiti e recupero osservabili |
| RNF-03 | Risorse adattive e Core trasferibile | Benchmark sull'hardware iniziale e prova di migrazione |
| RNF-04 | Identità, autorizzazione, audit e isolamento | Prove di accesso negato, revoca e conferma critica |
| RNF-05 | Local-first e minimizzazione dei dati | Test di blocco del traffico cloud per payload vietati |
| RNF-06 | Offline per le capacità compatibili | Test con rete assente e successiva riconciliazione |
| RNF-07 | Osservabilità e manutenibilità | Correlazione end-to-end e istruzioni di diagnosi riproducibili |
| RNF-08 | Backup, ripristino e rollback | Restore su ambiente pulito e rollback di release |

Per ogni requisito registrare milestone, componente, test, riferimento alla revisione, esito e stato. Gli stati sono Pianificato, In corso, Implementato, Verificato e Bloccato. L'accettazione finale segue [DEVOPS_QA_OBSERVABILITY.md](DEVOPS_QA_OBSERVABILITY.md).
