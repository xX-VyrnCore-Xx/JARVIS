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

**Dashboard (ADR-0023):** l'interfaccia è un'applicazione React e TypeScript (`apps/web`) con tema HUD olografico: scuro per impostazione, chiaro con il pulsante ◐. La vista iniziale **Plancia** riunisce:

**Rifinitura macOS e Windows (ADR-0035):** icone vettoriali, barra laterale a gruppi riducibile con il conteggio delle conferme, palette comandi (⌘K su macOS, Ctrl+K su Windows e Linux), riga di indicatori in Plancia, etichette e indicatore di scrittura in chat, notifiche con barra del tempo. La radice riceve `platform-mac` o `platform-win` per caratteri di sistema (SF, Segoe UI Variable), raggi e barre di scorrimento; l'app desktop apre la dashboard con `?app=desktop` in una finestra scura e mostra una schermata d'avvio animata.

**Icona e installatori (ADR-0036):** icona a reattore generata da `scripts/make_brand_assets.py`; `.dmg` con sfondo e istruzioni; setup Windows in italiano con immagini dedicate; su macOS barra del titolo trasparente e rimozione della quarantena del pacchetto dopo il primo avvio autorizzato.


- il nucleo animato di JARVIS, che segue gli stati pronto, elaborazione, ascolto, trascrizione e risposta vocale;
- ora, stato di AI, voce, audit e Core, e un comando rapido che apre la chat;
- le operazioni in attesa di conferma, approvabili o rifiutabili dalla plancia;
- le attività recenti con stato ed esecutore (Core locale, nodo, automazione, dispositivo);
- condizioni e nodi: Core, nodi esecutori in linea o no, modalità offline dell'AI, voce o Web non disponibili;
- prestazioni (p95 della chat, richieste, errori e storico), errori e avvisi recenti;
- memoria (fatti, preferenze, episodi, relazioni e grafo), automazioni attive e dispositivi.

Sul dispositivo associato la plancia non compare: si apre la chat.

L'interfaccia Web è servita dal Core su `http://localhost:8765` e comprende:
- chat con le risposte del modello locale che compaiono parola per parola (ADR-0025) e i passaggi di ogni risposta (agente, strumento, policy, esito);
- schede di conferma con effetto atteso, destinazione e scadenza;
- sezioni Attività, Automazioni, Memoria, Workspace e Audit. Nella sezione Automazioni si crea un'automazione da un modulo generato dal catalogo e la si può eseguire subito, sospendere, riattivare, eliminare o consultarne la cronologia;
- stato del backend AI e dell'integrità dell'audit nell'intestazione.

**Accesso OAuth ai connettori (ADR-0027):** un connettore con `auth.type = "oauth2"` mostra «Accedi». La pagina di consenso del servizio si apre nel browser di sistema; al ritorno JARVIS salva i token cifrati e li rinnova da solo. Nel servizio va registrato il redirect `http://localhost:8765/api/connectors/oauth/callback`.

**Notifiche sul telefono (ADR-0026):** dalla sezione Dispositivi si aggiunge un canale **ntfy** (app push gratuita, server `ntfy.sh` o il tuo) oppure **Telegram** (bot creato con @BotFather). Per ogni canale si scelgono la gravità minima (predefinita: avvisi e conferme) e se includere il testo. Senza consenso escono solo titolo e categoria. Con `JARVIS_PUBLIC_URL=https://…` il messaggio contiene il collegamento a JARVIS. Il pulsante «Prova» invia un messaggio di verifica. Le conferme si danno solo da JARVIS.

La campanella nell'intestazione mostra le notifiche non lette. Il pannello elenca categoria, ora e task correlato, e un clic porta alla vista collegata. Le nuove notifiche compaiono come toast e, se autorizzate, come notifiche del browser quando la pagina non è in primo piano.

**App desktop (ADR-0009):** `./jarvis.sh desktop` apre JARVIS in una finestra Tauri per Windows, macOS e Linux. L'app avvia il Core in automatico, tiene i dati nella cartella dell'applicazione dell'utente, apre i link esterni nel browser di sistema e alla chiusura arresta Core e worker, anche in caso di crash. La Plancia desktop usa superfici dedicate nei temi chiaro e scuro, tipografia di sistema e animazioni compatibili con la preferenza di movimento ridotto (ADR-0051).

La sezione **Sistema** mostra lo stato di Core, AI, voce, Web, audit e worker, e la tabella delle latenze (p50, p95, max, errori) per endpoint, strumento e delega.

Si adatta al tema chiaro e scuro e allo schermo dello smartphone (verificata a 390 px).

**Telefono e altri dispositivi (ADR-0014):**
1. Pubblica JARVIS nella tua rete privata con un proxy HTTPS fidato, per esempio `tailscale serve 8765`.
2. Avvia JARVIS con `JARVIS_ALLOWED_HOSTS=<nome-host>`.
3. Sul telefono apri l'indirizzo e inserisci il codice generato nella sezione **Dispositivi**.

L'interfaccia si installa come app («Aggiungi a schermata Home»). Dal dispositivo sono disponibili chat, attività, notifiche, voce e, se concesso, le conferme. La revoca è immediata. Se il Core o il proxy è irraggiungibile durante la navigazione, la PWA mostra una pagina con «Riprova». Al ritorno della connessione una scheda già aperta aggiorna stato e notifiche. Il service worker evita di salvare pagine, risposte o credenziali.

La Plancia e la Chat mostrano il ritratto maschile di JARVIS. Il pulsante **Parla con JARVIS** avvia il microfono dalla Plancia e permette di inviare la registrazione o interrompere una risposta. Il colore segue la categoria dell’attività, con un’etichetta leggibile e indicazioni specifiche per conferme ed errori. Gli stati vocali indicano ascolto, trascrizione e riproduzione. L’indicatore di voce segue l’intensità dell’audio locale e rispetta la preferenza per il movimento ridotto.

**Controlli PC.** Su macOS e Windows, dalla Chat locale il proprietario può aprire la cartella di JARVIS, la Calcolatrice, le impostazioni di sistema e il monitor delle attività. Può scegliere il comando dal menu **Controlli PC** oppure dire o scrivere «Apri la calcolatrice». Ogni richiesta viene eseguita dopo la conferma del proprietario, anche quando nella Chat è selezionato un provider cloud. I dispositivi associati e le automazioni non possono avviare queste azioni.

**Client Android (ADR-0018):** `apps/android` contiene l'app Jetpack Compose con associazione, cronologia, chat e conferme secondo i permessi. Segue le stesse regole del client iOS e conserva la sessione cifrata con l'Android Keystore. La CI produce l'APK di debug.

**Nodi esecutori (ADR-0017):** un altro computer associato con un codice «Nodo esecutore» esegue gli strumenti che dichiara (informazioni di sistema, ora, file della propria workspace) quando il Core glieli assegna. Usa `./jarvis.sh node <indirizzo> <codice>` e richieste in uscita, senza aprire porte. Nella sezione Dispositivi compaiono stato in linea, strumenti e comando da usare in chat.

**Configurazione guidata (ADR-0019):** al primo avvio una finestra propone voce e modello AI con le scelte consigliate già selezionate. Mostra la dimensione del download e l'avanzamento, e si riapre dalla sezione Sistema.

**Programmi di installazione (ADR-0016):** la release contiene l'app desktop pronta da installare. Windows `.msi` e setup `.exe`, macOS `.dmg`, Linux `.deb` e `.AppImage`. Core, runtime AI e Python sono inclusi.

Con «nodo auto <strumento>» il Core sceglie da solo il nodo più libero fra quelli in linea (ADR-0031).

Le app iOS e Android hanno tre schede: Chat, Attività e Notifiche (ADR-0032).

Restano da sviluppare: voce nelle app native, firma degli installer

## App nativa iOS (RF-10, RF-11)

Su macOS, apri `apps/ios/JARVIS.xcodeproj` in Xcode e avvia il target JARVIS su un simulatore iOS 17+ o su un iPhone/iPad. Per il dispositivo fisico seleziona il team di sviluppo in Xcode. Inserisci l'origine HTTPS già autorizzata per il Core, un nome e il codice monouso generato nella sezione **Dispositivi**.

Il client mostra i 50 messaggi più recenti, invia messaggi e mostra le conferme pendenti. Le schede Attività e Notifiche mostrano lo stato delle richieste e le notifiche, con «Segna lette» (ADR-0032). Il permesso `confirm` abilita approvazione e rifiuto espliciti. Il token resta nel portachiavi del dispositivo. Il client usa una sessione HTTP effimera, rifiuta i reindirizzamenti e torna alla schermata di associazione quando una richiesta autenticata restituisce 401. L'uscita locale cancella la sessione dal portachiavi. Per revocare il token sul Core usa la sezione **Dispositivi**.

La CI compila l'app per simulatore ed esegue i test Swift del protocollo. Le prove sul telefono usano la checklist in [ACCEPTANCE.md](ACCEPTANCE.md).

## Voce (RF-09, ADR-0010)

- **Push-to-talk:** il pulsante 🎤 avvia e chiude la registrazione e diventa rosso finché il microfono ascolta.
- **Stato visibile:** l'intestazione mostra lo stato della voce (pronta, in ascolto, trascrizione, risposta vocale, non disponibile con il motivo).
- **Risposte vocali** (opzionali): l'utente le interrompe con «Interrompi», con Esc o premendo il microfono.
- **Mani libere** (opzionale): mostra «● Microfono attivo» per tutto il tempo in cui il microfono è aperto. Si attiva con «Ehi Jarvis» e si interrompe parlando sopra la risposta.

Le conferme restano un clic.


**Nodi con mTLS (ADR-0041).** Avvia il Core con `JARVIS_NODE_TLS_ADDR=0.0.0.0:8766` (o l'indirizzo della rete privata), poi associa il nodo come sempre: riceve la porta e l'impronta del Core e da lì usa solo la connessione TLS reciproca. `JARVIS_NODE_TLS_URL` sul nodo sostituisce l'indirizzo se il Core è raggiungibile con un altro nome.


**Passkey sul telefono (ADR-0042).** Dalla chat del dispositivo associato: «Proteggi le approvazioni con una passkey». Dopo, «Approva» chiede impronta, volto o PIN. Serve HTTPS (es. `tailscale serve`). Il proprietario elimina una passkey con `DELETE /api/passkeys/{id}` dal computer del Core.


**App desktop nativa (ADR-0044).** Icona nella barra dei menu o nell'area di notifica, menu di macOS in italiano, ⌘⇧J / Ctrl+Maiusc+J per richiamare JARVIS, finestra che si nasconde alla chiusura, notifiche del sistema, posizione ricordata.
