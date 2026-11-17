# Interfacce e dispositivi

| Superficie | Funzioni |
|---|---|
| Desktop Windows, macOS e Linux | Chat, voce, dashboard, notifiche, integrazioni OS e gestione risorse locali |
| Web | Dashboard centralizzata, chat, task, memoria, automazioni, dispositivi, monitoraggio e impostazioni |
| Mobile Android/iOS | Interazione e controllo sui dispositivi con capacità compatibili, nelle milestone successive |
| Avatar | Identità visiva e vocale evolutiva, animazione e sincronizzazione labiale |
| Voce | Wake word, riconoscimento lingua, STT/TTS in streaming, full-duplex e interruzione |

La GUI è il punto di avvio principale. I flussi chat, task e notifiche condividono lo stato tramite API. La dashboard mostra attività, esecutore, condizioni offline, errori e operazioni in attesa di conferma. La voce rende visibili ascolto, elaborazione e riproduzione. Se un motore vocale manca o non è disponibile offline, l'utente può usare le capacità testuali disponibili.

## Ciclo di vita di un nodo

Registrare l'identità e dichiarare piattaforma, connettività, risorse e servizi autorizzati. Assegnare lavoro in base a capacità e policy. Visualizzare stato di connessione e sincronizzazione distinguendo attività in esecuzione, accodate e sospese. Alla riconnessione riconciliare progressi e dati come descritto in [MEMORY_DATA_SYNC.md](MEMORY_DATA_SYNC.md). Revocare identità e accessi quando il nodo viene rimosso.

Verificare per piattaforma enrollment, revoca, segnalazione dei limiti OS, ripresa della connessione, gestione dei permessi locali, interruzione vocale e stato coerente fra client.

## Stato del prototipo v0.1

L'interfaccia Web è servita dal Core su `http://localhost:8765` e comprende:
- chat, con i passaggi di ogni risposta (agente, strumento, policy, esito);
- schede di conferma con effetto atteso, destinazione e scadenza;
- sezioni Attività, Automazioni, Memoria, Workspace e Audit. Nella sezione Automazioni si crea un'automazione da un modulo generato dal catalogo e la si può eseguire subito, sospendere, riattivare, eliminare o consultarne la cronologia;
- stato del backend AI e dell'integrità dell'audit nell'intestazione.

La campanella nell'intestazione mostra le notifiche non lette. Il pannello elenca categoria, ora e task correlato, e un clic porta alla vista collegata. Le nuove notifiche compaiono come toast e, se autorizzate, come notifiche del browser quando la pagina non è in primo piano.

**App desktop (ADR-0009):** `./jarvis.sh desktop` apre JARVIS in una finestra Tauri per Windows, macOS e Linux. L'app avvia il Core in automatico, tiene i dati nella cartella dell'applicazione dell'utente, apre i link esterni nel browser di sistema e alla chiusura arresta Core e worker, anche in caso di crash.

Si adatta al tema chiaro e scuro e allo schermo dello smartphone (verificata a 390 px). Restano da sviluppare: installatori autonomi del desktop, voce, avatar e mobile nativo.
