## Novità in JARVIS 1.13.0

- Supporto opzionale a XTTS v2 locale con campione WAV fornito dal proprietario. Faster Whisper usa filtri e parole chiave mirate, con correzioni italiane conservative. La dettatura si può rivedere prima dell’invio al cloud.
- Verifica statica ed esecuzione Python con conferma in Docker isolato. Gli agenti temporanei usano strumenti di lettura limitati. Il terminale host richiede un consenso esplicito.
- Budget del contesto cloud e controllo di credenziali riconoscibili prima dell’invio. Il workflow pubblica dal commit di `main` dopo una CI verde.

## Installazione

| Sistema | File | Primo avvio |
|---|---|---|
| **macOS** 11 o successivo | `JARVIS_*_aarch64.dmg` (chip Apple) oppure `JARVIS_*_x64.dmg` (Intel) | Apri il `.dmg` e trascina JARVIS in **Applicazioni**. Al primo avvio macOS avvisa che l'app non è firmata da Apple: **Impostazioni di Sistema › Privacy e sicurezza › Apri comunque**. Se compare «JARVIS è danneggiato», esegui nel Terminale `xattr -cr /Applications/JARVIS.app` e riapri. |
| **Windows** 10/11 (64 bit) | `JARVIS_*_x64-setup.exe` | Se SmartScreen avvisa: **Ulteriori informazioni › Esegui comunque**. Il setup installa per l'utente corrente, senza permessi di amministratore. |

Al primo avvio si apre la configurazione guidata. Verifica dell'integrità: `SHA256SUMS` è firmato con Sigstore (`SHA256SUMS.sigstore.json`), ogni installer ha l'attestazione di provenienza di GitHub.
