## Novità in JARVIS 1.12.0

- **Conversazioni con contesto.** In Chat, «Continua la conversazione» riusa fino a tre scambi testuali con lo stesso provider. «Qual è la fonte?» ripropone i collegamenti verificati del turno precedente. Per i riferimenti ambigui, JARVIS chiede chiarimenti.
- **Modelli cloud.** Ollama Cloud e NVIDIA NIM gestiscono la conversazione libera. Configura le chiavi in **Sistema → Modelli cloud**. NIM ha un limite di 35 richieste al minuto per Core. Voce e comandi a regole sono disponibili offline.
- **Strumenti e automazioni.** Il Core verifica le proposte dei modelli e richiede una conferma per le scritture. L’avatar interattivo offre controlli vocali, il Centro sicurezza mostra lo stato del Core e n8n gestisce routine locali per promemoria e audit.

## Installazione

| Sistema | File | Primo avvio |
|---|---|---|
| **macOS** 11 o successivo | `JARVIS_*_aarch64.dmg` (chip Apple) oppure `JARVIS_*_x64.dmg` (Intel) | Apri il `.dmg` e trascina JARVIS in **Applicazioni**. Al primo avvio macOS avvisa che l'app non è firmata da Apple: **Impostazioni di Sistema › Privacy e sicurezza › Apri comunque**. Se compare «JARVIS è danneggiato», esegui nel Terminale `xattr -cr /Applications/JARVIS.app` e riapri. |
| **Windows** 10/11 (64 bit) | `JARVIS_*_x64-setup.exe` (consigliato) oppure `.msi` | Se SmartScreen avvisa: **Ulteriori informazioni › Esegui comunque**. Il setup installa per l'utente corrente, senza permessi di amministratore. |
| **Linux** | `.deb` oppure `.AppImage`; ARM64: `JARVIS_*_arm64.deb` | `sudo apt install ./JARVIS_*_amd64.deb` (o `_arm64.deb`), oppure rendi eseguibile l'AppImage. |

Al primo avvio si apre la configurazione guidata. Verifica dell'integrità: `SHA256SUMS` è firmato con Sigstore (`SHA256SUMS.sigstore.json`), ogni installer ha l'attestazione di provenienza di GitHub.
