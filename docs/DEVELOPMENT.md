# Sviluppo nel monorepo

Il repository GitHub privato e il mirror locale conservano codice, documenti e checkpoint. In radice stanno README.md come indice e CLAUDE.md come istruzioni per Claude Code; le altre 12 specifiche stanno in `docs/` (scelta del proprietario, commit `d482b80`). La disposizione seguente indica le directory già presenti e la proposta per i moduli futuri. Adeguarla al repository effettivo quando esistono già confini coerenti.

```text
/
├── README.md, CLAUDE.md
├── docs/              specifiche, registro decisioni e checkpoint (ROADMAP.md)
├── core/              Core Linux e componenti Rust (workspace Cargo)
│   ├── contracts/     crate jarvis-contracts: validazione dei contratti v1
│   ├── bridge/        crate jarvis-bridge: prototipo Core–worker Python
│   └── server/        crate jarvis-core: binario `jarvis` e API locale
├── ai/                runtime agenti e adapter Python (jarvis_ai)
├── apps/
│   ├── desktop/       launcher/ (crate jarvis-launcher) e src-tauri/ (app nativa egui)
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

## Client desktop nativo

L'interfaccia è un'app Rust/egui in `apps/desktop/src-tauri`; il Core espone solo API e non serve pagine, bundle o service worker Web. Il client desktop richiede Rust 1.95 o successivo. Su Linux installa le librerie grafiche prima della build:

```bash
sudo apt install libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libgl1-mesa-dev libdbus-1-dev
cd apps/desktop/src-tauri
cargo fmt --check && cargo clippy --locked -- -D warnings && cargo test --locked
```

Il test trasversale `tests/e2e/api_smoke.cjs` verifica il Core HTTP, l'autenticazione, il pairing e lo streaming senza browser. `contracts.yml` compila e testa il client nativo su Linux, macOS e Windows.

## Dipendenze della voce

`ai/requirements-voice.in` elenca le dipendenze dirette; `ai/requirements-voice.txt` è il lock con tutte le dipendenze transitive e i rispettivi hash SHA-256. Per aggiornarlo, usa Python 3.13 e `pip-tools==7.7.0`, poi esegui:

```bash
pip-compile --generate-hashes --output-file=ai/requirements-voice.txt ai/requirements-voice.in
```

Le installazioni di sviluppo, il setup guidato e le release usano `pip install --require-hashes`; ogni aggiornamento al lock va verificato sui pacchetti Windows e macOS in CI.
Il lock include anche le dipendenze condizionali specifiche di Windows (per esempio `colorama` richiesta da `tqdm`); verificare il lock su Windows oltre che sul sistema che lo genera.

Le GitHub Actions nei workflow sono fissate a commit SHA. Aggiorna i riferimenti solo dopo aver verificato la release upstream e il commit corrispondente.

## Ripresa da remoto con Claude Code

Aprire la radice del checkout autorizzato. Verificare che branch e revisione corrispondano al checkpoint atteso. Scoprire comandi, dipendenze e servizi dai manifest e dalla configurazione del repository, registrando eventuali prerequisiti mancanti. Il lavoro riprendibile deve poter essere ricostruito da cronologia Git, documenti e verifiche, anche quando una nuova sessione non ha la cronologia della chat. Centralizzare le istruzioni condivise nel repository e mantenere i segreti nelle configurazioni autorizzate dell'ambiente.

Prima di chiudere, distinguere modifiche nel checkout, commit locali, revisioni condivise e verifiche effettivamente eseguite. Rispettare le autorizzazioni della sessione per sincronizzazioni remote e pubblicazione.

## Integrazione delle modifiche

Il proprietario ha autorizzato, il 2026-11-17, il merge automatico delle pull request di Claude Code quando tutti i job della CI (`.github/workflows/contracts.yml`) sono verdi. Il merge resta vietato con CI rossa, conflitti o revisioni che chiedono modifiche. Dopo il merge il branch di lavoro riparte da `main`, avanzando senza riscrivere la storia.

### Pubblicazione automatica dopo il merge

Il workflow `contracts` verifica ogni push, compresi quelli su `main`. `release-after-ci.yml` avvia la pubblicazione solo dopo il completamento verde di `contracts` per un push sul `main` dello stesso repository. La release viene costruita dallo SHA verificato; il numero dell’esecuzione assegna un tag univoco senza modificare il branch. Windows x64 e macOS Apple Silicon devono superare build, installazione e prova di avvio prima della pubblicazione.

Dopo la pubblicazione, `scripts/prune_releases.py` conserva la release con la versione SemVer più alta e rimuove le altre release e i rispettivi tag, comprese le prerelease. L’ordinamento SemVer evita che una build più lenta di un merge precedente elimini una release successiva. Il rilancio sullo stesso SHA e tag è idempotente. Il workflow usa `gh release create --notes ... --generate-notes` per aggiungere le note del progetto alle note automatiche di GitHub.

### Firma degli installer

Le release firmano DMG e app macOS e setup Windows con un certificato autofirmato privato, senza
acquisti o account commerciali. Il proprietario deve configurare i certificati e i secret elencati
in [GITHUB_ACTIONS_SECRETS.md](./GITHUB_ACTIONS_SECRETS.md); se mancano, la pubblicazione fallisce
in modo esplicito. La CI verifica la firma, ma l'utente deve considerare attendibile il certificato
sui propri dispositivi. Non viene eseguita la notarizzazione Apple.

Le PR verificano build e installazione senza usare i certificati privati. Sigstore e le attestazioni
di provenienza continuano a usare OIDC GitHub.
