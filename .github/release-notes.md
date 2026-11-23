## Installazione

| Sistema | File | Come si installa |
|---|---|---|
| **macOS Apple Silicon** (macOS 14 o successivo) | `JARVIS_*_aarch64.dmg` | Apri il DMG e trascina JARVIS in **Applicazioni**. All'avvio e dopo ogni blocco l'app chiede Touch ID o la password del Mac. |
| **Windows x64** (Windows 10 o 11) | `JARVIS_*_x64-setup.exe` | Avvia il setup: installa per l'utente corrente, senza permessi di amministratore. |
| **Android** (telefono associato al Core) | `JARVIS_*_android-release.apk` | Consenti l'installazione da questa origine, apri l'APK e associa il telefono con il codice monouso creato in **Dispositivi**. |

Al primo avvio si apre la configurazione guidata. Il Core resta sul tuo computer: i telefoni lo raggiungono solo tramite il tuo proxy HTTPS privato.

## Verifica prima di installare

Ogni file ha un'attestazione di provenienza di GitHub e una firma Sigstore (`.sigstore.json`). `SHA256SUMS` elenca le impronte ed è firmato a sua volta.

<details>
<summary>Comandi di verifica</summary>

```bash
# impronta: deve comparire nell'elenco firmato
sha256sum -c SHA256SUMS --ignore-missing

# provenienza: il file è stato costruito dal workflow di questo repository
gh attestation verify <file> --repo xX-VyrnCore-Xx/JARVIS

# firma dell'elenco delle impronte
cosign verify-blob --bundle SHA256SUMS.sigstore.json \
  --certificate-identity-regexp 'https://github.com/xX-VyrnCore-Xx/JARVIS/.github/workflows/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com SHA256SUMS
```

</details>

Gli installer desktop sono firmati con i certificati configurati nei GitHub Actions Secret: se il sistema avvisa che l'editore non è noto, confronta prima impronta e attestazione come sopra. Le modifiche di questa versione sono elencate qui sotto.
