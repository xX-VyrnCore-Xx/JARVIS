# Interfacce e dispositivi

| Superficie | Funzioni |
|---|---|
| Desktop Windows, macOS e Linux | App nativa Rust/egui, chat streaming, dati del Core e notifiche OS |
| Mobile Android | App Jetpack Compose, associazione, chat, attività e notifiche push |
| Core | API HTTP autenticate; non distribuisce pagine o bundle Web |

Il client iOS e l'interfaccia Web/PWA sono stati ritirati. L'associazione e la gestione dei permessi continuano a usare le API Core: non è disponibile una pagina di pairing nel browser.

## App desktop nativa

`./jarvis.sh desktop` compila e avvia l'app egui dai sorgenti; gli installer includono GUI, Core, Python e runtime AI. Il client può avviare il Core locale oppure associarsi a un Core remoto HTTPS con invito monouso. Le credenziali remote vengono conservate nella cartella dati dell'utente con permessi restrittivi.

La finestra usa una barra superiore e una navigazione laterale fisse; lo scorrimento è confinato al contenuto centrale. A larghezze strette la navigazione si compatta, mentre chat e barra di composizione restano visibili. Le sezioni disponibili sono Plancia, Chat, Attività, Automazioni, Memoria, Competenze, Workspace, Connettori, Dispositivi, Sicurezza, Audit e Sistema.

La chat usa `/api/chat/stream` (NDJSON), mostra le risposte progressivamente e presenta le conferme per approvazione o rifiuto. Il client aggiorna le notifiche ogni cinque secondi anche quando la finestra non è in primo piano e invia le nuove notifiche alle API OS supportate da Linux, macOS e Windows. Gli errori di aggiornamento sono mostrati nell'interfaccia. Il Core continua a generare e conservare notifiche anche senza client connesso.

La Plancia desktop usa una gerarchia di schede con saluto, avatar originale animato, accessi rapidi e riepiloghi reali di Core, AI, conferme, notifiche, automazioni e audit. L'animazione è discreta e si può ridurre dalla Plancia. Le notifiche recenti mantengono azioni e gestione errori. La navigazione e l'intestazione restano fisse; lo scorrimento è confinato al contenuto anche nelle finestre strette. La vista usa i colori del tema scuro e la tipografia nativa di egui su Windows, macOS e Linux, senza dipendere da servizi o immagini remote.

Le viste Attività, Automazioni, Memoria, Competenze, Workspace, Connettori, Dispositivi, Sicurezza, Audit e Sistema leggono le rispettive API. Sono già disponibili cronologia chat, correzione/eliminazione della memoria, esecuzione e attivazione delle automazioni, inviti e revoca dei dispositivi, e creazione di backup. La UI usa ancora una vista JSON generica per varie risposte: non equivale alla parità funzionale con la precedente dashboard.

## App Android e notifiche

`apps/android` contiene il client Jetpack Compose con pairing HTTPS, Plancia, cronologia, chat, attività e notifiche. La home mobile mostra un avatar originale animato, collegamento alla chat, conteggi di attività e notifiche e l'ultima attività disponibili al dispositivo associato; il riepilogo è aggiornato dal Core e rende visibili gli errori. L'animazione rispetta la scala di movimento del sistema. La barra inferiore resta ancorata al viewport e il contenuto della scheda scorre internamente. La sessione usa Android Keystore; il client rifiuta i reindirizzamenti. Quando l'app è in background, il Core invia notifiche FCM data-only ad alta priorità se il proprietario ha configurato Firebase e l'utente ha abilitato notifiche e permesso OS. Il polling si sospende quando l'app non è in primo piano. Il testo delle notifiche resta generico per ridurre l'esposizione dei dati.

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
