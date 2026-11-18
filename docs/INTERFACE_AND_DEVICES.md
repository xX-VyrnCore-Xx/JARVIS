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

La sezione **Sistema** mostra lo stato di Core, AI, voce, Web, audit e worker, e la tabella delle latenze (p50, p95, max, errori) per endpoint, strumento e delega.

Si adatta al tema chiaro e scuro e allo schermo dello smartphone (verificata a 390 px).

**Telefono e altri dispositivi (ADR-0014):**
1. Pubblica JARVIS nella tua rete privata con un proxy HTTPS fidato, per esempio `tailscale serve 8765`.
2. Avvia JARVIS con `JARVIS_ALLOWED_HOSTS=<nome-host>`.
3. Sul telefono apri l'indirizzo e inserisci il codice generato nella sezione **Dispositivi**.

L'interfaccia si installa come app («Aggiungi a schermata Home»). Dal dispositivo sono disponibili chat, attività, notifiche, voce e, se concesso, le conferme. La revoca è immediata. Se il Core o il proxy è irraggiungibile durante la navigazione, la PWA mostra una pagina con «Riprova». Al ritorno della connessione una scheda già aperta aggiorna stato e notifiche. Il service worker evita di salvare pagine, risposte o credenziali.

La chat mostra un avatar astratto con anelli CSS 3D. L'etichetta annuncia gli stati pronto, elaborazione, ascolto, trascrizione e risposta vocale. L'avatar segue l'effettivo avvio della riproduzione. Durante la riproduzione l'apertura della bocca segue l'intensità dell'audio locale e rispetta la preferenza per il movimento ridotto.

**Programmi di installazione (ADR-0016):** la release contiene l'app desktop pronta da installare. Windows `.msi` e setup `.exe`, macOS `.dmg`, Linux `.deb` e `.AppImage`. Core, runtime AI e Python sono inclusi.

Restano da sviluppare: sincronizzazione labiale basata sui fonemi, client nativo Android, funzioni iOS oltre chat e conferme, firma degli installer e nodi esecutori distribuiti.

## App nativa iOS (RF-10, RF-11)

Su macOS, apri `apps/ios/JARVIS.xcodeproj` in Xcode e avvia il target JARVIS su un simulatore iOS 17+ o su un iPhone/iPad. Per il dispositivo fisico seleziona il team di sviluppo in Xcode. Inserisci l'origine HTTPS già autorizzata per il Core, un nome e il codice monouso generato nella sezione **Dispositivi**.

Il client mostra i 50 messaggi più recenti, invia messaggi e mostra le conferme pendenti. Il permesso `confirm` abilita approvazione e rifiuto espliciti. Il token resta nel portachiavi del dispositivo. Il client usa una sessione HTTP effimera, rifiuta i reindirizzamenti e torna alla schermata di associazione quando una richiesta autenticata restituisce 401. L'uscita locale cancella la sessione dal portachiavi. Per revocare il token sul Core usa la sezione **Dispositivi**.

La CI compila l'app per simulatore ed esegue i test Swift del protocollo. Le prove sul telefono usano la checklist in [ACCEPTANCE.md](ACCEPTANCE.md).

## Voce (RF-09, ADR-0010)

- **Push-to-talk:** il pulsante 🎤 avvia e chiude la registrazione e diventa rosso finché il microfono ascolta.
- **Stato visibile:** l'intestazione mostra lo stato della voce (pronta, in ascolto, trascrizione, risposta vocale, non disponibile con il motivo).
- **Risposte vocali** (opzionali): l'utente le interrompe con «Interrompi», con Esc o premendo il microfono.
- **Mani libere** (opzionale): mostra «● Microfono attivo» per tutto il tempo in cui il microfono è aperto. Si attiva con «Ehi Jarvis» e si interrompe parlando sopra la risposta.

Le conferme restano un clic.
