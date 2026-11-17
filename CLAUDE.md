# JARVIS — Istruzioni per Claude Code

Comando di ripresa dalla radice del monorepo:

> Leggi CLAUDE.md e riprendi JARVIS dalla prima attività verificabile della milestone corrente.

## Orientamento

Leggi [README.md](README.md), [ROADMAP.md](ROADMAP.md), [DEVELOPMENT.md](DEVELOPMENT.md) e [DECISIONS.md](DECISIONS.md). Controlla branch, commit, working tree, manifest, struttura, test disponibili e checkpoint. Verifica che il checkpoint descriva la revisione effettiva. Poi leggi requisiti, architettura e documenti dei sottosistemi interessati.

## Esecuzione

- Ricostruisci quanto è implementato dalle evidenze disponibili. Identifica una prossima attività concreta con requisito, dipendenze e criteri verificabili.
- Riusa moduli e contratti presenti. Conserva le modifiche locali altrui e mantieni il confine fra Core Rust, AI Python, applicazioni e infrastruttura.
- Procedi nel perimetro autorizzato. Quando opportuno, usa subagent per analisi o compiti circoscritti e verifica risultati e integrazione.
- Verifica le modifiche con i comandi effettivi del repository. Registra comandi ed esiti. Se manca un servizio, un segreto o una capacità dell'host remoto, indica quale controllo è rimasto sospeso.
- Mantieni i controlli di identità, permessi, privacy, audit e conferma applicabili a ogni nuova capacità. Usa fixture sintetiche per le verifiche.
- Per 75–80 conserva le voci senza contenuto fino al recupero delle fonti. Per 81–83 segui le risposte documentate. Se una lacuna impedisce una scelta con effetti sul progetto, lavoraci attorno dove possibile e registra la decisione da recuperare.

## Chiusura della sessione

Aggiorna il checkpoint in [ROADMAP.md](ROADMAP.md) con milestone, revisione, esiti, evidenze, blocchi e prossimo passo. Aggiorna le specifiche coinvolte e registra le decisioni in [DECISIONS.md](DECISIONS.md). Indica quali modifiche sono nel checkout e quali sono già disponibili sul branch condiviso. Usa solo le operazioni Git e remote autorizzate per la sessione.
