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
