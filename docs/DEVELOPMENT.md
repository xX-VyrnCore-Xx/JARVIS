# Sviluppo nel monorepo

Il repository GitHub privato e il mirror locale conservano codice, documenti e checkpoint. In radice stanno README.md come indice e CLAUDE.md come istruzioni per Claude Code; le altre 12 specifiche stanno in `docs/` (scelta del proprietario, commit `d482b80`). La disposizione seguente indica le directory già presenti e la proposta per i moduli futuri. Adeguarla al repository effettivo quando esistono già confini coerenti.

```text
/
├── README.md, CLAUDE.md
├── docs/              specifiche, registro decisioni e checkpoint (ROADMAP.md)
├── core/              Core Linux e componenti Rust (workspace Cargo)
│   ├── contracts/     crate jarvis-contracts: validazione dei contratti v1
│   ├── bridge/        crate jarvis-bridge: prototipo Core–worker Python
│   └── server/        crate jarvis-core: binario `jarvis`, API locale e interfaccia Web
├── ai/                runtime agenti e adapter Python (jarvis_ai)
├── apps/
│   ├── desktop/       launcher/ (crate jarvis-launcher) e src-tauri/ (app Tauri)
│   ├── web/           dashboard React e TypeScript (bundle in core/server/static)
│   └── mobile/        client successivi
├── contracts/         schemi, API ed eventi versionati; catalog/tools: descrittori degli strumenti
├── integrations/      plugin, connettori e SDK
├── migrations/        evoluzione dei dati
├── infra/             systemd, container e release
├── jarvis.sh          avvio del prototipo
└── tests/             prove trasversali e fixture sintetiche (contracts/, ai/, e2e/)
```

## Flusso di lavoro

1. Esaminare branch, revisione, modifiche locali, struttura e checkpoint. Conservare le modifiche già presenti.
2. Scegliere un risultato verificabile dalla milestone corrente. Collegare requisiti e dipendenze.
3. Aggiornare contratti e compatibilità insieme al codice che li usa. Tenere i cambiamenti coerenti e il diff leggibile.
4. Eseguire le verifiche già previste dal repository e aggiungere la copertura necessaria al cambiamento. Nei flussi dati provare almeno compatibilità, recovery e ripetizione quando rilevanti.
5. Registrare esiti reali, limiti, decisioni e prossimo passo in ROADMAP.md. Se cambia una scelta architetturale, aggiornare DECISIONS.md e la specifica associata.

## Dashboard (apps/web, ADR-0023)

La dashboard è in React e TypeScript. Il bundle generato (`core/server/static/app.js` e `app.css`) è versionato, così il Core si compila senza Node. Dopo una modifica in `apps/web/src`:

```bash
cd apps/web && npm ci && npm run build
```

La CI ricostruisce il bundle e fallisce se differisce da quello versionato. Gli ID e le classi usati da `tests/e2e/ui_smoke.cjs` fanno parte del contratto dell'interfaccia.

## Dipendenze della voce

`ai/requirements-voice.in` elenca le dipendenze dirette; `ai/requirements-voice.txt` è il lock con tutte le dipendenze transitive e i rispettivi hash SHA-256. Per aggiornarlo, usa Python 3.13 e `pip-tools==7.7.0`, poi esegui:

```bash
pip-compile --generate-hashes --output-file=ai/requirements-voice.txt ai/requirements-voice.in
```

Le installazioni di sviluppo, il setup guidato e le release usano `pip install --require-hashes`; ogni aggiornamento al lock va verificato sui pacchetti Windows e macOS in CI.
Il lock include anche le dipendenze condizionali specifiche di Windows (per esempio `colorama` richiesta da `tqdm`); verificare il lock su Windows oltre che sul sistema che lo genera.

Le GitHub Actions nei workflow sono fissate a commit SHA; la CLI Tauri ha una versione esatta. Aggiorna i riferimenti solo dopo aver verificato la release upstream e il commit corrispondente.

## Ripresa da remoto con Claude Code

Aprire la radice del checkout autorizzato. Verificare che branch e revisione corrispondano al checkpoint atteso. Scoprire comandi, dipendenze e servizi dai manifest e dalla configurazione del repository, registrando eventuali prerequisiti mancanti. Il lavoro riprendibile deve poter essere ricostruito da cronologia Git, documenti e verifiche, anche quando una nuova sessione non ha la cronologia della chat. Centralizzare le istruzioni condivise nel repository e mantenere i segreti nelle configurazioni autorizzate dell'ambiente.

Prima di chiudere, distinguere modifiche nel checkout, commit locali, revisioni condivise e verifiche effettivamente eseguite. Rispettare le autorizzazioni della sessione per sincronizzazioni remote e pubblicazione.

## Integrazione delle modifiche

Il proprietario ha autorizzato, il 2026-11-17, il merge automatico delle pull request di Claude Code quando tutti i job della CI (`.github/workflows/contracts.yml`) sono verdi. Il merge resta vietato con CI rossa, conflitti o revisioni che chiedono modifiche. Dopo il merge il branch di lavoro riparte da `main`, avanzando senza riscrivere la storia.

### Pubblicazione automatica dopo il merge

Il workflow `contracts` verifica ogni push, compresi quelli su `main`. `release-after-ci.yml` avvia la pubblicazione solo dopo il completamento verde di `contracts` per un push sul `main` dello stesso repository. La release viene costruita dallo SHA verificato; il numero dell’esecuzione assegna un tag univoco senza modificare il branch. Windows x64 e macOS Apple Silicon devono superare build, installazione e prova di avvio prima della pubblicazione.

Dopo la pubblicazione, `scripts/prune_releases.py` conserva la release con la versione SemVer più alta e rimuove le altre release e i rispettivi tag, comprese le prerelease. L’ordinamento SemVer evita che una build più lenta di un merge precedente elimini una release successiva. Il rilancio sullo stesso SHA e tag è idempotente. Il workflow usa `gh release create --notes ... --generate-notes` per aggiungere le note del progetto alle note automatiche di GitHub.

### Firma degli installer

La release automatica da `main` attesta e firma con Sigstore il DMG, il setup EXE e `SHA256SUMS`. Le firme sono bundle separati e vengono verificate prima della pubblicazione.

Per aggiungere le firme native, configura GitHub Actions e imposta la variabile `JARVIS_NATIVE_SIGNING=true` dopo aver predisposto entrambi i sistemi.

- **Apple.** Serve un certificato **Developer ID Application**. Aggiungi il P12 codificato Base64 a `APPLE_CERTIFICATE` e la password a `APPLE_CERTIFICATE_PASSWORD`. Imposta la variabile `APPLE_SIGNING_IDENTITY` con il nome esatto del certificato. Aggiungi ai secret `APPLE_API_KEY` (ID chiave di App Store Connect), `APPLE_API_ISSUER` (issuer ID) e `JARVIS_APPLE_API_KEY_BASE64` (contenuto del file P8 codificato Base64). Tauri firma l’app e il DMG. La CI verifica le firme e allega i ticket ottenuti dalla notarizzazione.
- **Windows.** Serve un profilo pubblico Azure Artifact Signing. Configura la federazione GitHub OIDC per il solo subject `repo:xX-VyrnCore-Xx/JARVIS:ref:refs/heads/main` e assegna il ruolo **Artifact Signing Certificate Profile Signer** al profilo. Imposta le variabili `AZURE_SIGNING_CLIENT_ID`, `AZURE_SIGNING_TENANT_ID`, `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT` e `AZURE_SIGNING_PROFILE`. La CI verifica Authenticode e la marca temporale prima della prova d’installazione.

Le PR compilano i pacchetti con la configurazione di test. Se abiliti la firma nativa per la pubblicazione automatica, un errore di firma impedisce la pubblicazione.
