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

## Ripresa da remoto con Claude Code

Aprire la radice del checkout autorizzato. Verificare che branch e revisione corrispondano al checkpoint atteso. Scoprire comandi, dipendenze e servizi dai manifest e dalla configurazione del repository, registrando eventuali prerequisiti mancanti. Il lavoro riprendibile deve poter essere ricostruito da cronologia Git, documenti e verifiche, anche quando una nuova sessione non ha la cronologia della chat. Centralizzare le istruzioni condivise nel repository e mantenere i segreti nelle configurazioni autorizzate dell'ambiente.

Prima di chiudere, distinguere modifiche nel checkout, commit locali, revisioni condivise e verifiche effettivamente eseguite. Rispettare le autorizzazioni della sessione per sincronizzazioni remote e pubblicazione.

## Integrazione delle modifiche

Il proprietario ha autorizzato, il 2026-11-17, il merge automatico delle pull request di Claude Code quando tutti i job della CI (`.github/workflows/contracts.yml`) sono verdi. Il merge resta vietato con CI rossa, conflitti o revisioni che chiedono modifiche. Dopo il merge il branch di lavoro riparte da `main`, avanzando senza riscrivere la storia.

### Pubblicazione automatica dopo il merge

Il workflow `contracts` verifica ogni push su `main`. Dopo il successo, `release-after-ci.yml` chiama `release.yml` sullo SHA verificato. Per assegnare una versione univoca, somma il numero dell’esecuzione CI alla patch dichiarata e applica il risultato alla build del Core e degli installer, senza modificare il branch. La revisione riceve il tag dopo i test di installazione del setup Windows x64 e del DMG Apple Silicon. La pubblicazione automatica mantiene i tag e gli installer delle altre revisioni e il rilancio sullo stesso SHA con lo stesso numero CI è idempotente.
