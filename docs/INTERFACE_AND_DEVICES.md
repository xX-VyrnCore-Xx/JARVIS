# Interfacce e dispositivi

## Client macOS SwiftUI

Su macOS 14 o successivo, JARVIS usa un client SwiftUI con autenticazione del proprietario all’apertura. Il Core locale parte dopo la verifica e si arresta quando termina la finestra. Minimizzare la finestra o riattivare il processo mantiene lo stato corrente. Su Windows la build usa egui. La plancia SwiftUI presenta stato e contatori del Core, attività recenti e accesso a chat e conferme. L’avatar visualizza l’attività del Core. Sintesi e trascrizione sono sospese finché non viene integrato un provider audio cloud.


| Superficie | Funzioni |
|---|---|
| Desktop macOS e Windows | App nativa Rust/egui, chat streaming, dati del Core e notifiche OS |
| Mobile Android | App Jetpack Compose, associazione, chat, attività e notifiche push |
| Core headless (Linux incluso) | API HTTP autenticate; non distribuisce pagine o bundle Web |

Il client iOS e l'interfaccia Web/PWA sono stati ritirati. L'associazione e la gestione dei permessi continuano a usare le API Core: non è disponibile una pagina di pairing nel browser.

## App desktop nativa

Il client egui dai sorgenti si avvia con `./jarvis.sh desktop` su macOS o `.\jarvis.ps1 desktop` su Windows; gli installer includono GUI, Core, Python e runtime AI. Il DMG apre una finestra Finder impaginata con JARVIS.app e il collegamento Applications affiancati e le istruzioni visibili; il wizard Windows ricorda la cartella scelta, crea collegamenti in Start e sul Desktop e offre l'avvio facoltativo al termine. La disinstallazione Windows elimina l'app, non i dati dell'utente. Il client può avviare il Core locale oppure associarsi a un Core remoto HTTPS con invito monouso. Da **Plancia → Core collegati** si possono associare ulteriori Core con inviti separati. Le credenziali di ogni Core sono conservate nella configurazione desktop dell'utente con permessi restrittivi; dimenticare un Core rimuove la credenziale locale, mentre la revoca dell'autorizzazione si esegue nella sezione Dispositivi del Core interessato.

L'icona originale usa un monogramma J luminoso e leggibile anche alle dimensioni ridotte; il generatore disegna il marchio senza immagini raster esterne e produce PNG, ICO, ICNS, asset Android e grafiche degli installer. Il marchio è condiviso da finestra, DMG, collegamenti Windows e launcher Android. Durante l'avvio una schermata di benvenuto mostra lo stato della connessione locale o remota; se il Core non parte, l'errore è visibile e si può riprovare senza chiudere l'app.

La finestra usa una barra superiore e una navigazione laterale fisse; lo scorrimento è confinato al contenuto centrale. A larghezze strette la navigazione si compatta, mentre chat e barra di composizione restano visibili. Le sezioni disponibili sono Plancia, Chat, Attività, Automazioni, Memoria, Competenze, Workspace, Connettori, Dispositivi, Sicurezza, Audit e Sistema.

La chat usa `/api/chat/stream` (NDJSON), mostra le risposte progressivamente e presenta le conferme per approvazione o rifiuto. Il client aggiorna le notifiche ogni cinque secondi anche quando la finestra non è in primo piano e invia le nuove notifiche alle API OS supportate da macOS e Windows. Gli errori di aggiornamento sono mostrati nell'interfaccia. Il Core continua a generare e conservare notifiche anche senza client connesso.

La Plancia desktop usa un riquadro di benvenuto a tre zone nelle finestre ampie: sintesi dei Core e dell'AI, avatar vettoriale animato e azioni rapide. Nelle finestre medie le azioni passano sotto il riquadro; in quelle strette introduzione, avatar e comandi si dispongono in verticale. La barra superiore compatta evita sovrapposizioni e mantiene accessibili notifiche e indicatore del Core. Le schede di stato hanno un accento cromatico semantico e si dispongono in tre, due o una colonna; attività e notifiche sono affiancate solo quando la larghezza lo consente. Le attività recenti usano schede compatte con stato localizzato e colore semantico, metadati secondari e un'azione esplicita per preparare una bozza in chat. Non vengono mostrate risorse hardware, progetti o agenti senza dati API attendibili.

Se sono associati più Core, riepiloghi e attività recenti sono aggregati solo in questa vista: richieste parallele con timeout breve producono risultati parziali e indicano i Core non raggiungibili. I conteggi di un Core offline o con salute non disponibile, e le attività recenti quando la relativa richiesta fallisce, restano sconosciuti e sono mostrati come «—», non come zeri. I dati della risposta di salute vengono ridotti ai soli campi di riepilogo. Chat, viste dettagliate, notifiche interattive e azioni continuano a usare il Core attivo; i Core aggiunti non ricevono chat né mutazioni. L'elenco delle notifiche mostra il Core attivo, mentre il conteggio non lette somma i Core raggiungibili.

L'avatar è una figura vettoriale geometrica a più livelli con visiera, dettagli luminosi, collarino e armatura; il ritratto desktop arriva a 280 punti e quello Android occupa 248 dp in altezza. L'anello HUD ha tacche e indicatori in orbita, mentre una scansione luminosa attraversa la visiera e la figura fluttua lievemente. Non usa fotografie o risorse remote. Il tema scuro è applicato indipendentemente dal tema del sistema; «Riduci animazioni» ferma il moto desktop e Android rispetta la scala animazioni del sistema. Navigazione, pulsanti e indicatore del Core condividono bordi, stati di selezione e spaziature coerenti su Windows e macOS; l'indicatore distingue Core locale e remoto. Le notifiche recenti distinguono categoria e gravità, consentono di aprire solo le viste permesse e di segnare una singola notifica come letta; quelle OS mostrano anche gravità e categoria. Intestazione e navigazione restano fisse; lo scorrimento è confinato al contenuto anche nelle finestre strette. La vista usa la tipografia nativa di egui su macOS e Windows, senza dipendere da servizi o immagini remote.

Le viste Attività, Automazioni, Memoria, Competenze, Workspace, Connettori, Dispositivi, Sicurezza, Audit e Sistema leggono le rispettive API. L'autorizzazione OAuth apre il browser solo per un URL HTTPS privo di credenziali incorporate; al rientro si aggiorna la sezione Connettori per verificare lo stato. Risposte OAuth senza un URL sicuro producono un errore visibile. Memoria consente la ricerca testuale e la visualizzazione di tutti i ricordi, mantenendo correzione ed eliminazione sul Core proprietario. La vista Dispositivi consente di scegliere fra gli host HTTPS configurati e copiare istruzioni con host selezionato, codice temporaneo e scope. Sono inoltre disponibili cronologia chat, esecuzione e attivazione delle automazioni, revoca dei dispositivi e creazione di backup. La UI usa ancora una vista JSON generica per varie risposte: non equivale alla parità funzionale con la precedente dashboard.

## App Android e notifiche

`apps/android` contiene il client Jetpack Compose con pairing HTTPS, Plancia, cronologia, chat, attività e notifiche. Desktop e Android che usano lo stesso Core condividono cronologia e stato lato Core. La home mobile mostra un avatar originale animato, collegamento alla chat, conteggi di attività e notifiche e l'ultima attività disponibili al dispositivo associato; il riepilogo è aggiornato dal Core e rende visibili gli errori. **Prepara la ripresa in chat** trasferisce l'obiettivo dell'attività come bozza, senza inviarla: l'utente può modificarla e scegliere se inviarla. Le notifiche mantengono il collegamento alla vista associata; il tap su un push apre e mette in evidenza la notifica nel client. Chat, attività e Plancia hanno navigazione diretta; per le altre viste consentite dal contratto ma non supportate nell'app Android compare un'indicazione esplicita, senza aprire una schermata diversa o ampliare gli scope. Le attività collegate vengono cercate nell'elenco recente e messe in evidenza. L'animazione rispetta la scala di movimento del sistema. La barra inferiore resta ancorata al viewport e il contenuto della scheda scorre internamente. La sessione usa Android Keystore; il client rifiuta i reindirizzamenti. Quando l'app è in background, il Core invia notifiche FCM data-only ad alta priorità se il proprietario ha configurato Firebase e l'utente ha abilitato notifiche e permesso OS. Categoria e gravità sono mostrate in italiano, mentre il contenuto del push resta generico per ridurre l'esposizione dei dati; l'ID notifica è usato solo per focalizzare l'elemento dopo la riapertura, senza trasmettere il titolo, il testo o il riferimento del task nel payload push. Il polling si sospende quando l'app non è in primo piano.

Configura Firebase seguendo [FIREBASE_SETUP.md](FIREBASE_SETUP.md). Il workflow allega l'APK release firmato (mai debug) alla stessa release degli installer desktop; la verifica reale delle push richiede un progetto Firebase e un dispositivo autorizzati.

Android adotta un aspetto in stile iOS (ADR-0101): palette di sistema, titolo grande, schede arrotondate, barra a schede e fumetti come in iMessage. Con l'app aperta le notifiche arrivano in tempo reale dal Core (ADR-0098); in background restano le push FCM.

Sul Mac l'interfaccia si blocca dopo 5 minuti di inattività, allo stop e al blocco dello schermo, e si riapre con Touch ID o la password; il Core continua a lavorare. Dispositivi, chiavi cloud, connettori, backup, passkey, aggiornamenti e canali di notifica chiedono di nuovo l'identità (ADR-0100). La sezione Missioni (macOS e Windows) permette di affidare obiettivi a JARVIS e di regolare le iniziative proattive.

## Associazione e sicurezza

Per usare il Mac come host condiviso mantenendo il Core legato a `127.0.0.1:8765`, Tailscale Serve fornisce HTTPS privato all'interno della tailnet:

1. Installa Tailscale sul Mac e sui client e collegali alla stessa tailnet. Avvia il Core JARVIS sul Mac.
2. Nel Terminale del Mac configura il reverse proxy HTTPS:

   ```bash
   tailscale serve --bg 8765
   tailscale serve status
   ```

   Se richiesto, approva l'abilitazione dei certificati HTTPS nelle impostazioni della tailnet. Usa l'indirizzo HTTPS mostrato da `serve status`. Serve inoltra a `127.0.0.1:8765` e rende il servizio disponibile ai dispositivi autorizzati nella tailnet; non è necessario aprire porte sul router. Non usare **Funnel**, che espone un servizio a Internet. Per opzioni e comportamento aggiornati, consulta la [documentazione ufficiale di Tailscale Serve](https://tailscale.com/docs/features/tailscale-serve).
3. Autorizza il nome host nel Core, senza schema, porta o percorso: aggiungilo a `remote-hosts.txt` nella cartella dati del Core, una voce per riga, oppure imposta `JARVIS_ALLOWED_HOSTS=<host>`. Per l'app desktop macOS installata la cartella predefinita è `~/Library/Application Support/JARVIS`; se `JARVIS_DATA_DIR` è configurato, usa quella cartella. Riavvia JARVIS perché la lista è letta all'avvio.
4. Nella sezione **Dispositivi** del Mac seleziona l’indirizzo e usa **Verifica HTTPS**. Genera un invito monouso e scegli se concedere anche le conferme. Sul client Android o desktop inserisci l'origine HTTPS (`https://<host-mac>.ts.net`, sostituita con l'indirizzo effettivo) e il codice. Il codice scade dopo dieci minuti e può essere annullato.

Su Android la schermata di associazione permette di ripetere la verifica prima di inviare il codice. Entrambi i client usano la fiducia TLS del sistema e un endpoint che restituisce solo l’identificativo e la versione del protocollo di connessione. La verifica rifiuta i reindirizzamenti. Le passkey usano l’origine HTTPS canonica del proxy, compresa l’eventuale porta non standard.

Il client deve essere connesso a Tailscale. Per controllare o rimuovere le configurazioni Serve usa `tailscale serve status` e `tailscale serve reset`; il reset rimuove tutte le condivisioni Serve configurate su quel Mac. Come alternativa, un altro reverse proxy privato e fidato può terminare TLS e inoltrare solo al loopback.

Le conferme e le altre operazioni restano limitate agli scope assegnati. I dispositivi associati non possono gestire inviti, audit, backup o connettori. La revoca ha effetto immediato. I nodi esecutori continuano a usare `./jarvis.sh node <indirizzo> <codice>` e richieste in uscita.

## Funzioni da completare

- La UI desktop nativa non ha ancora microfono/STT/TTS, wake word, schermate guidate per i connettori e la configurazione cloud, né viste tipizzate complete per sicurezza, audit e sistema.
- La finestra desktop non offre ancora il comportamento tray/nascondi della precedente app Tauri.
- Restano da verificare la consegna FCM su hardware reale e l'aspetto/permessi delle notifiche OS sui runner e sistemi desktop supportati.

Queste lacune non riattivano né richiedono il client Web o iOS; vanno completate nei rispettivi client nativi mantenendo policy, scope, conferme e audit del Core.
