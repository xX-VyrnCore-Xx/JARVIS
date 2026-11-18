## Installazione

| Sistema | File | Primo avvio |
|---|---|---|
| **macOS** (Apple Silicon, macOS 11 o successivo) | `JARVIS_*_aarch64.dmg` | Apri il `.dmg` e trascina JARVIS in **Applicazioni**. Al primo avvio macOS avvisa che l'app non è firmata da Apple: **Impostazioni di Sistema › Privacy e sicurezza › Apri comunque**. Se compare «JARVIS è danneggiato», esegui nel Terminale `xattr -cr /Applications/JARVIS.app` e riapri. |
| **Windows** 10/11 (64 bit) | `JARVIS_*_x64-setup.exe` (consigliato) oppure `.msi` | Se SmartScreen avvisa: **Ulteriori informazioni › Esegui comunque**. Il setup installa per l'utente corrente, senza permessi di amministratore. |
| **Linux** | `.deb` oppure `.AppImage` | `sudo apt install ./JARVIS_*_amd64.deb`, oppure rendi eseguibile l'AppImage. |

Al primo avvio si apre la configurazione guidata. Verifica dell'integrità: `SHA256SUMS` è firmato con Sigstore (`SHA256SUMS.sigstore.json`), ogni installer ha l'attestazione di provenienza di GitHub.
