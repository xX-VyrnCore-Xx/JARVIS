# Configurazione privata di GitHub Actions

Apri **Settings → Secrets and variables → Actions** nel repository `xX-VyrnCore-Xx/JARVIS`.

Le release desktop preferiscono i certificati nei Secrets:
`JARVIS_MACOS_SIGNING_P12_BASE64` con `JARVIS_MACOS_SIGNING_PASSWORD`, e
`JARVIS_WINDOWS_SIGNING_PFX_BASE64` con `JARVIS_WINDOWS_SIGNING_PASSWORD`.
Se i Secret sono assenti o non validi, il workflow genera automaticamente un certificato
autofirmato temporaneo sul runner. Non serve configurare account Apple/Azure né eseguire
passaggi manuali. L'APK Android usa separatamente i quattro Secret Android.

## Firma APK Android

Genera un keystore Android persistente; la chiave deve rimanere la stessa per aggiornare l'app:

```bash
keytool -genkeypair -v -keystore "$HOME/.jarvis/signing/jarvis-release.jks" \
  -alias jarvis-release -keyalg RSA -keysize 3072 -validity 10000
base64 < "$HOME/.jarvis/signing/jarvis-release.jks" | tr -d '\n'
```

Il valore Base64 va in `ANDROID_KEYSTORE_BASE64` (Secret). Aggiungi anche `ANDROID_KEYSTORE_PASSWORD`,
`ANDROID_KEY_ALIAS` (nell'esempio `jarvis-release`) e `ANDROID_KEY_PASSWORD` (Secrets). Conserva
una copia cifrata del keystore e delle password. La pipeline rifiuta di pubblicare un APK non
firmato.

## Firebase Android

Il file `google-services.json` fornito corrisponde al package `local.jarvis.app`. I soli identificativi
Firebase Android richiesti dal build non sono credenziali server. Dalla radice del checkout esegui:

```bash
python3 scripts/configure_firebase_actions.py google-services.json
```

Lo script trasferisce i quattro campi necessari direttamente a **Actions → Variables**, senza
stamparne i valori: `JARVIS_FIREBASE_API_KEY`, `JARVIS_FIREBASE_PROJECT_ID`,
`JARVIS_FIREBASE_APP_ID` e `JARVIS_FIREBASE_SENDER_ID`. Richiede `gh auth login` e autorizzazione
`Actions: write` sul repository. Quando lo script termina con successo, `google-services.json` non
serve più e puoi eliminarlo dal checkout. Una configurazione parziale viene rifiutata dalla pipeline.

La credenziale JSON del service account **non** va in GitHub Actions: resta sul computer che ospita
il Core, con accesso limitato al suo utente e `JARVIS_FCM_SERVICE_ACCOUNT_FILE` impostata al runtime;
vedi [FIREBASE_SETUP.md](./FIREBASE_SETUP.md). Il file `client_secret_*.json` OAuth non è usato
dall'app Android, da FCM o dai workflow. Può servire per Gmail/Google Calendar OAuth e va gestito
separatamente dal Core, mai come secret di build o nel repository.

## Nessun altro secret per le Actions

La firma Sigstore e le attestazioni GitHub usano OIDC e non richiedono chiavi persistenti. Non
configurare `APPLE_CERTIFICATE`, chiavi di notarizzazione, Azure Artifact Signing o credenziali
OAuth di Google nelle Actions.

Il token GitHub per leggere le release quando il repository è privato **non** è un secret Actions:
configuralo dalla sezione **Aggiornamenti** dell'app desktop. Il Core lo conserva nel proprio secret
store cifrato; per una repository pubblica lascia il campo vuoto.
