# Configurazione Firebase Cloud Messaging

JARVIS invia le notifiche push dal Core ai dispositivi Android associati usando FCM HTTP v1. Le
notifiche contengono solo un avviso generico e un identificativo: non includono il titolo o il testo
privato dell'evento.

## Progetto Firebase

1. Crea un progetto Firebase dedicato o seleziona quello JARVIS.
2. Nella console Firebase registra un'app Android con package name `local.jarvis.app`.
3. Abilita la Firebase Cloud Messaging API nel progetto Google Cloud collegato.
4. Il file locale `google-services.json` può essere passato a
   `python3 scripts/configure_firebase_actions.py google-services.json`: lo script valida il
   package e inserisce questi quattro identificativi nelle Actions variables senza stamparli:
   - Web API key → `JARVIS_FIREBASE_API_KEY`
   - Project ID → `JARVIS_FIREBASE_PROJECT_ID`
   - App ID Android → `JARVIS_FIREBASE_APP_ID`
   - Cloud Messaging Sender ID → `JARVIS_FIREBASE_SENDER_ID`
   In alternativa, in GitHub apri **Settings → Secrets and variables → Actions → Variables** e
   aggiungi tutti e quattro i valori con gli stessi nomi. Sono identificativi/configurazione client
   usati dall'SDK, non credenziali server. Non riutilizzare qui la chiave privata del service account.

Il file `google-services.json` non è necessario dopo aver configurato le Actions variables; resta
ignorato da Git e può essere eliminato dal checkout. Il file OAuth `client_secret_*.json` non è una
credenziale FCM e non va caricato nelle Actions.

La build Android legge gli stessi valori da variabili d'ambiente o proprietà Gradle con prefisso
`jarvisFirebase` (per esempio `JARVIS_FIREBASE_PROJECT_ID` o
`-PjarvisFirebaseProjectId=...`). Senza i quattro valori l'APK è costruibile, ma l'attivazione push
resta disabilitata. La release GitHub rifiuta una configurazione parziale.

## Credenziale server sul Core

1. In Google Cloud crea un service account dedicato al progetto e assegnagli solo i permessi
   necessari per inviare messaggi FCM (ruolo **Firebase Cloud Messaging API Admin**).
2. Genera una chiave JSON solo se il deployment non può usare credenziali di workload/ambiente.
3. Copia il JSON fuori dal repository, in un percorso leggibile solo dall'utente del Core
   (permessi `0600` su Unix).
4. Avvia il Core con `JARVIS_FCM_SERVICE_ACCOUNT_FILE=/percorso/protetto/service-account.json`.
   Il `project_id` del file deve coincidere con il progetto dell'app Android.
5. Riavvia JARVIS e verifica lo stato push in **Sistema**/log. Se il file manca o non è valido,
   il Core registra un errore esplicito e non invia il push.

Non caricare il JSON del service account nel repository, nei log o nelle Actions di build. Il Core
usa questa credenziale al runtime per ottenere un token OAuth breve; non è necessaria per compilare
l'APK.

## Verifica su Android

Installa la release APK, associa il dispositivo a un Core raggiungibile via HTTPS, concedi il permesso
di notifiche e attiva **Notifiche → Attiva notifiche push**. Genera una notifica di prova dal Core,
poi ripeti con JARVIS ridotto in background. Android o il produttore possono limitare le notifiche
se l'utente forza l'arresto dell'app o nega il permesso.

Le credenziali reali Firebase sono specifiche del proprietario: i test CI validano la compilazione e
il formato dei messaggi con fixture sintetiche, non l'invio a un telefono reale.
