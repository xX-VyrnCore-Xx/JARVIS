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
credenziale FCM e non va caricato nelle Actions. Può però servire per autorizzare i connettori
separati Gmail/Google Calendar: in tal caso configura l'ID client nel connettore e conserva il
client secret nell'archivio cifrato del Core, dopo il consenso esplicito agli scope richiesti.

## Connettori Gmail e Google Calendar

Per collegare l'account Google con il client OAuth desktop, imposta temporaneamente un token Core
conosciuto nei due terminali locali:

```bash
mkdir -p data
umask 077
python3 -c 'import secrets; print(secrets.token_hex(32))' > data/.google-oauth-token
export JARVIS_TOKEN="$(cat data/.google-oauth-token)"
./jarvis.sh
```

In un secondo terminale, dalla stessa radice:

```bash
export JARVIS_TOKEN="$(cat data/.google-oauth-token)"
python3 scripts/configure_google_connectors.py "$HOME/.jarvis/google/client_secret.json"
```

Lo script accetta solo un Core HTTP su loopback, installa i manifest Gmail e Calendar, invia il
client secret al Core localmente e apre i due flussi Google nel browser. Conferma personalmente
gli scope richiesti: Gmail `gmail.readonly`/`gmail.compose` e Calendar `calendar.readonly`/
`calendar.events`. Il Core conserva i token OAuth nel proprio archivio cifrato; Google deve
riconoscere il tuo account come utente di test se il progetto OAuth è in modalità Testing. Il Core
aggiunge `access_type=offline` per ricevere un refresh token e usa il redirect loopback richiesto da
Google Desktop (`http://127.0.0.1:<porta>`, senza percorso). La radice `/` resta 404 salvo quando
riceve il `state` di un callback OAuth valido. Non eseguire questo flusso su un Core remoto o non
fidato.

Dopo che entrambi i connettori risultano autorizzati, arresta il Core e rimuovi il token temporaneo e
il file client OAuth dal checkout:

```bash
rm -- data/.google-oauth-token "$HOME/.jarvis/google/client_secret.json"
```

La chiave privata OAuth non va committata né registrata nelle GitHub Actions. Se i connettori sono
già autorizzati, il Core non ha bisogno del file client per continuare a usare i token cifrati.

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
