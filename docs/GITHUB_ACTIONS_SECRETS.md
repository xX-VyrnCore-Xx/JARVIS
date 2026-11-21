# Secrets e variabili GitHub Actions

Apri **Settings → Secrets and variables → Actions** nel repository `xX-VyrnCore-Xx/JARVIS`.
Questa lista descrive i nomi richiesti; non inserire i valori nel repository o in questo documento.
I secrets sono scritti solo nell'ambiente autorizzato delle Actions.

## Necessari per pubblicare la release Android

| Nome | Tipo | Contenuto |
|---|---|---|
| `ANDROID_KEYSTORE_BASE64` | Secret | Keystore Android di release, codificato Base64 |
| `ANDROID_KEYSTORE_PASSWORD` | Secret | Password del keystore |
| `ANDROID_KEY_ALIAS` | Secret | Alias della chiave di firma |
| `ANDROID_KEY_PASSWORD` | Secret | Password della chiave |

Conserva una copia cifrata e sicura del keystore: senza la stessa chiave non puoi aggiornare
l'installazione Android già distribuita.

## Firebase Android (facoltativo per compilare, necessario al push)

Questi sono identificativi della configurazione client Firebase, non segreti server. Aggiungili in
**Variables**, non in Secrets:

| Nome | Tipo | Contenuto |
|---|---|---|
| `JARVIS_FIREBASE_API_KEY` | Variable | Web API key della configurazione Firebase Android |
| `JARVIS_FIREBASE_PROJECT_ID` | Variable | Project ID |
| `JARVIS_FIREBASE_APP_ID` | Variable | App ID Android |
| `JARVIS_FIREBASE_SENDER_ID` | Variable | Sender ID |

Vanno configurate tutte e quattro oppure nessuna. Per i dettagli del service account al runtime,
vedi [FIREBASE_SETUP.md](./FIREBASE_SETUP.md). Il file JSON server non è un secret usato dal job di
compilazione APK.

## Firma nativa degli installer desktop (opzionale)

### Apple Developer ID e notarizzazione

| Nome | Tipo | Contenuto |
|---|---|---|
| `APPLE_CERTIFICATE` | Secret | Certificato Developer ID `.p12` codificato Base64 |
| `APPLE_CERTIFICATE_PASSWORD` | Secret | Password del `.p12` |
| `APPLE_API_KEY` | Secret | ID della chiave App Store Connect |
| `APPLE_API_ISSUER` | Secret | Issuer ID App Store Connect |
| `JARVIS_APPLE_API_KEY_BASE64` | Secret | File `.p8` della chiave App Store Connect codificato Base64 |
| `APPLE_SIGNING_IDENTITY` | Variable | Nome esatto dell'identità Developer ID |

### Windows Artifact Signing

La federazione OIDC non richiede un client secret. Configura queste **Variables** e l'identità
federata Azure con subject limitato a `repo:xX-VyrnCore-Xx/JARVIS:ref:refs/heads/main`:

| Nome | Tipo | Contenuto |
|---|---|---|
| `AZURE_SIGNING_CLIENT_ID` | Variable | Client ID dell'app Azure federata |
| `AZURE_SIGNING_TENANT_ID` | Variable | Tenant ID |
| `AZURE_SIGNING_ENDPOINT` | Variable | Endpoint Artifact Signing |
| `AZURE_SIGNING_ACCOUNT` | Variable | Nome account Artifact Signing |
| `AZURE_SIGNING_PROFILE` | Variable | Profilo certificato |
| `JARVIS_NATIVE_SIGNING` | Variable | `true` solo dopo aver configurato entrambe le firme native |

La firma Sigstore della release usa OIDC GitHub e non richiede chiavi persistenti. Con `JARVIS_NATIVE_SIGNING`
non attivo, la pubblicazione usa i pacchetti non firmati nativamente.
