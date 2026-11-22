# Interfacce e dispositivi

| Superficie | Funzioni |
|---|---|
| Desktop macOS e Windows | App nativa Rust/egui, chat streaming, dati del Core e notifiche OS |
| Mobile Android | App Jetpack Compose, associazione, chat, attività e notifiche push |
| Core headless (Linux incluso) | API HTTP autenticate; non distribuisce pagine o bundle Web |

Il client iOS e l'interfaccia Web/PWA sono stati ritirati. L'associazione e la gestione dei permessi continuano a usare le API Core: non è disponibile una pagina di pairing nel browser.

## App desktop nativa

Il client egui dai sorgenti si avvia con `./jarvis.sh desktop` su macOS o `.\jarvis.ps1 desktop` su Windows; gli installer includono GUI, Core, Python e runtime AI. Il DMG apre una finestra Finder impaginata con JARVIS.app e il collegamento Applications affiancati e le istruzioni visibili; il wizard Windows ricorda la cartella scelta, crea collegamenti in Start e sul Desktop e offre l'avvio facoltativo al termine. La disinstallazione Windows elimina l'app, non i dati dell'utente. Il client può avviare il Core locale oppure associarsi a un Core remoto HTTPS con invito monouso. Le credenziali remote vengono conservate nella cartella dati dell'utente con permessi restrittivi.

L'icona originale vettoriale ha risorse ottimizzate per PNG, ICO e ICNS; la stessa immagine è impostata sulla finestra e usata dal DMG e dai collegamenti Windows. Durante l'avvio una schermata di benvenuto mostra lo stato della connessione locale o remota; se il Core non parte, l'errore è visibile e si può riprovare senza chiudere l'app.

La finestra usa una barra superiore e una navigazione laterale fisse; lo scorrimento è confinato al contenuto centrale. A larghezze strette la navigazione si compatta, mentre chat e barra di composizione restano visibili. Le sezioni disponibili sono Plancia, Chat, Attività, Automazioni, Memoria, Competenze, Workspace, Connettori, Dispositivi, Sicurezza, Audit e Sistema.

La chat usa `/api/chat/stream` (NDJSON), mostra le risposte progressivamente e presenta le conferme per approvazione o rifiuto. Il client aggiorna le notifiche ogni cinque secondi anche quando la finestra non è in primo piano e invia le nuove notifiche alle API OS supportate da macOS e Windows. Gli errori di aggiornamento sono mostrati nell'interfaccia. Il Core continua a generare e conservare notifiche anche senza client connesso.

La Plancia desktop usa una gerarchia di schede a tutta larghezza con saluto, avatar originale animato, accessi rapidi e riepiloghi reali di Core, AI, conferme, notifiche, automazioni e audit. L'avatar è una figura vettoriale geometrica a più livelli con profilo, dettagli luminosi e armatura; non usa fotografie o risorse remote. I riepiloghi hanno colori di stato e si dispongono in tre, due o una colonna secondo la larghezza; attività e notifiche sono affiancate solo nelle finestre ampie. Il tema scuro è applicato indipendentemente dal tema del sistema; il riquadro iniziale mantiene un'altezza contenuta. L'animazione è discreta e si può ridurre dalla Plancia. Le notifiche recenti distinguono categoria e gravità, consentono di aprire solo le viste permesse e di segnare una singola notifica come letta; quelle OS mostrano anche gravità e categoria. La navigazione e l'intestazione restano fisse; lo scorrimento è confinato al contenuto anche nelle finestre strette. La vista usa la tipografia nativa di egui su macOS e Windows, senza dipendere da servizi o immagini remote.

Le viste Attività, Automazioni, Memoria, Competenze, Workspace, Connettori, Dispositivi, Sicurezza, Audit e Sistema leggono le rispettive API. Memoria consente la ricerca testuale e la visualizzazione di tutti i ricordi, mantenendo correzione ed eliminazione sul Core proprietario. La vista Dispositivi consente di scegliere fra gli host HTTPS configurati e copiare istruzioni con host selezionato, codice temporaneo e scope. Sono inoltre disponibili cronologia chat, esecuzione e attivazione delle automazioni, revoca dei dispositivi e creazione di backup. La UI usa ancora una vista JSON generica per varie risposte: non equivale alla parità funzionale con la precedente dashboard.

## App Android e notifiche

`apps/android` contiene il client Jetpack Compose con pairing HTTPS, Plancia, cronologia, chat, attività e notifiche. La home mobile mostra un avatar originale animato, collegamento alla chat, conteggi di attività e notifiche e l'ultima attività disponibili al dispositivo associato; il riepilogo è aggiornato dal Core e rende visibili gli errori. L'animazione rispetta la scala di movimento del sistema. La barra inferiore resta ancorata al viewport e il contenuto della scheda scorre internamente. La sessione usa Android Keystore; il client rifiuta i reindirizzamenti. Quando l'app è in background, il Core invia notifiche FCM data-only ad alta priorità se il proprietario ha configurato Firebase e l'utente ha abilitato notifiche e permesso OS. Il tap apre direttamente la scheda notifiche; da lì si può segnare letta una notifica singola o l'intero elenco. Categoria e gravità sono mostrate in italiano, mentre il contenuto del push resta generico per ridurre l'esposizione dei dati. Il polling si sospende quando l'app non è in primo piano.

Configura Firebase seguendo [FIREBASE_SETUP.md](FIREBASE_SETUP.md). Il workflow allega l'APK release firmato (mai debug) alla stessa release degli installer desktop; la verifica reale delle push richiede un progetto Firebase e un dispositivo autorizzati.

## Associazione e sicurezza

1. Esporre il Core solo tramite un proxy HTTPS privato e fidato, ad esempio `tailscale serve 8765`.
2. Configurare `JARVIS_ALLOWED_HOSTS=<nome-host>` e riavviare il Core.
3. Dalla sezione Dispositivi del client desktop locale generare un invito monouso; inserirlo nel client desktop remoto o Android. Il codice scade dopo dieci minuti e può essere annullato.

Le conferme e le altre operazioni restano limitate agli scope assegnati. I dispositivi associati non possono gestire inviti, audit, backup o connettori. La revoca ha effetto immediato. I nodi esecutori continuano a usare `./jarvis.sh node <indirizzo> <codice>` e richieste in uscita.

## Funzioni da completare

- La UI desktop nativa non ha ancora microfono/STT/TTS, wake word, schermate guidate per i connettori e la configurazione cloud, né viste tipizzate complete per sicurezza, audit e sistema.
- La finestra desktop non offre ancora il comportamento tray/nascondi della precedente app Tauri.
- Restano da verificare la consegna FCM su hardware reale e l'aspetto/permessi delle notifiche OS sui runner e sistemi desktop supportati.

Queste lacune non riattivano né richiedono il client Web o iOS; vanno completate nei rispettivi client nativi mantenendo policy, scope, conferme e audit del Core.
