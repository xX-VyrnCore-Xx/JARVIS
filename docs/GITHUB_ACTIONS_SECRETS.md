# Configurazione privata di GitHub Actions

Apri **Settings → Secrets and variables → Actions** nel repository `xX-VyrnCore-Xx/JARVIS`.
Non inserire chiavi o certificati nel repository. Le firme native delle release sono autofirmate:
non servono certificati commerciali Apple o Azure. Windows e macOS mostreranno comunque avvisi
finché il proprietario non considera attendibile il certificato sui propri dispositivi.

## Firma privata di Windows e macOS

Genera una sola identità autofirmata e custodisci chiave privata e password fuori dal repository.
Su macOS o Linux, con OpenSSL:

```bash
mkdir -p "$HOME/.jarvis/signing"
chmod 700 "$HOME/.jarvis/signing"
cd "$HOME/.jarvis/signing"
umask 077
openssl req -x509 -newkey rsa:3072 -sha256 -days 3650 -nodes \
  -keyout jarvis-code-signing.key -out jarvis-code-signing.crt \
  -subj "/CN=JARVIS Private Code Signing" \
  -addext "basicConstraints=critical,CA:FALSE" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=codeSigning"
openssl pkcs12 -export -out jarvis-code-signing.p12 \
  -inkey jarvis-code-signing.key -in jarvis-code-signing.crt \
  -name "JARVIS Private Code Signing"
cp jarvis-code-signing.p12 jarvis-code-signing.pfx
openssl x509 -in jarvis-code-signing.crt -noout -fingerprint -sha1
```

Imposta i seguenti **Secrets**. L'input Base64 viene letto da file senza mostrarne il contenuto:

```bash
base64 < "$HOME/.jarvis/signing/jarvis-code-signing.p12" | tr -d '\n' |
  gh secret set JARVIS_MACOS_SIGNING_P12_BASE64 --repo xX-VyrnCore-Xx/JARVIS
base64 < "$HOME/.jarvis/signing/jarvis-code-signing.pfx" | tr -d '\n' |
  gh secret set JARVIS_WINDOWS_SIGNING_PFX_BASE64 --repo xX-VyrnCore-Xx/JARVIS
```

Imposta anche `JARVIS_MACOS_SIGNING_PASSWORD` e `JARVIS_WINDOWS_SIGNING_PASSWORD` come Secrets
tramite l'interfaccia GitHub (puoi riutilizzare la stessa password). Imposta come **Variable**
`JARVIS_MACOS_SIGNING_IDENTITY` il fingerprint SHA-1 del certificato, copiato senza `:`. Le release
firmano l'app e il DMG macOS e il setup Windows, poi verificano le firme. Non viene eseguita la
notarizzazione Apple né usato un servizio di firma commerciale.

Per consentire l'avvio sui tuoi dispositivi, importa `jarvis-code-signing.crt` come certificato
attendibile per la firma del codice nel Portachiavi macOS e in **Certificati - Utente corrente -
Autorità di certificazione radice attendibili** su Windows. Fallo solo sui tuoi dispositivi e
conserva una copia offline di `.key`, `.p12` e `.pfx`: chi possiede la chiave privata può firmare
programmi con l'identità JARVIS. La firma autofirmata non fornisce reputazione SmartScreen né
notarizzazione Apple; macOS può richiedere l'autorizzazione manuale al primo avvio. Se perdi la
chiave, genera un certificato nuovo e aggiorna tutti i secrets/variables e i dispositivi attendibili.

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
dall'app Android, da FCM o dai workflow.

## Nessun altro secret per le Actions

La firma Sigstore e le attestazioni GitHub usano OIDC e non richiedono chiavi persistenti. Non
configurare `APPLE_CERTIFICATE`, chiavi di notarizzazione, Azure Artifact Signing o credenziali
OAuth di Google nelle Actions.
