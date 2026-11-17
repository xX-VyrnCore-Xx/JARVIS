# Sviluppo nel monorepo

Il repository GitHub privato e il mirror locale conservano codice, documenti e checkpoint. Tenere in radice i 14 documenti, con README.md come indice e CLAUDE.md come istruzioni per Claude Code. La disposizione seguente è una proposta operativa per i moduli futuri. Adeguarla al repository effettivo quando esistono già confini coerenti.

```text
/
├── README.md, CLAUDE.md e altre specifiche
├── core/              Core Linux e componenti Rust
├── ai/                runtime agenti e adapter Python
├── apps/
│   ├── desktop/       Tauri, React e TypeScript
│   ├── web/           dashboard React
│   └── mobile/        client successivi
├── contracts/         schemi, API ed eventi versionati
├── integrations/      plugin, connettori e SDK
├── migrations/        evoluzione dei dati
├── infra/             systemd, container e release
└── tests/             prove trasversali e fixture sintetiche
```

## Flusso di lavoro

1. Esaminare branch, revisione, modifiche locali, struttura e checkpoint. Conservare le modifiche già presenti.
2. Scegliere un risultato verificabile dalla milestone corrente. Collegare requisiti e dipendenze.
3. Aggiornare contratti e compatibilità insieme al codice che li usa. Tenere i cambiamenti coerenti e il diff leggibile.
4. Eseguire le verifiche già previste dal repository e aggiungere la copertura necessaria al cambiamento. Nei flussi dati provare almeno compatibilità, recovery e ripetizione quando rilevanti.
5. Registrare esiti reali, limiti, decisioni e prossimo passo in ROADMAP.md. Se cambia una scelta architetturale, aggiornare DECISIONS.md e la specifica associata.

## Ripresa da remoto con Claude Code

Aprire la radice del checkout autorizzato. Verificare che branch e revisione corrispondano al checkpoint atteso. Scoprire comandi, dipendenze e servizi dai manifest e dalla configurazione del repository, registrando eventuali prerequisiti mancanti. Il lavoro riprendibile deve poter essere ricostruito da cronologia Git, documenti e verifiche, anche quando una nuova sessione non ha la cronologia della chat. Centralizzare le istruzioni condivise nel repository e mantenere i segreti nelle configurazioni autorizzate dell'ambiente.

Prima di chiudere, distinguere modifiche nel checkout, commit locali, revisioni condivise e verifiche effettivamente eseguite. Rispettare le autorizzazioni della sessione per sincronizzazioni remote e pubblicazione.
