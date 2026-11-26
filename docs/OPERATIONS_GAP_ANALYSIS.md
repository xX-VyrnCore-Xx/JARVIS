# Audit: operazioni digitali e interazione uomo-macchina (ADR-0185)

Data: 2026-11-25. Metodo: elenco di strumenti del catalogo (`contracts/catalog/tools`), moduli del Core (`core/server/src`), rotte (`api.rs`) e app macOS. **Audit per ispezione del codice, non per esecuzione**: dove non ho provato il comportamento lo scrivo. Una voce è IMPLEMENTED solo se il codice esiste e ha test; non conta la documentazione.

| § | Area | Stato | Evidenza / lacuna |
|---|---|---|---|
| 3 | Controllo del computer | PARTIAL | `app.open/focus/windows/type/key`, `clipboard.*` (conferma, macOS/Windows). Mancano mouse, multi-monitor, desktop virtuali, albero di accessibilità, coordinate schermo. Nessuna simulazione/arresto immediato dedicati. |
| 4 | Visione | PARTIAL | `screen.read` (cattura + OCR Tesseract locale, file cancellato, testo mascherato). Mancano analisi di immagini/grafici, confronto visivo, rilevamento di cambiamenti, aree dello schermo. Serve un modello con visione: BLOCKED finché non è scelto e autorizzato il provider. |
| 5 | Voce | PARTIAL | Wake word, trascrizione, sintesi, motore di sistema macOS (ADR-0183). Da verificare: barge-in, eco, VAD, dizionario di pronuncia. |
| 6 | Workspace intelligente | PARTIAL → **nuovo** | Profili con app e cartelle e piano di attivazione (questa sessione). Mancano snapshot, ripristino sessioni, modalità focus. |
| 7 | Centro operativo personale | PARTIAL | `todo.*`, `notes.*`, briefing giornaliero/settimanale (`/api/proactive/*`), connettori Calendar/Gmail. Mancano obiettivi, blocchi di tempo, tracciamento sessioni. |
| 8 | Intelligenza per sviluppatori | PARTIAL → **migliorato** | `git.*`, `deps.list`, `log.analyze`, `code.check_python`, `code.map` (simboli, grafo delle dipendenze, impatto; ADR-0189). Mancano indice persistente/incrementale, copertura, diagnosi build. |
| 9 | Infrastruttura | PARTIAL | `system.*`, `process.list`, `it.subnet`, `/api/docker`, nodi e dispositivi, Tailscale. Mancano inventario server, SSH, certificati, uptime, backup remoti. |
| 10 | Documenti | PARTIAL → **migliorato** | `pdf.extract`, `sheet.read`, `table.summary`, `text.diff`, `doc.read` (DOCX, PPTX, testo, con provenienza SHA-256) e `doc.compare` (ADR-0193). Mancano OCR, versioni salvate, ricerca semantica, generazione di documenti. |
| 11 | Comunicazioni | PARTIAL | Connettori Gmail/Calendar, canali di notifica. Bozze separate dall'invio: da verificare. |
| 12 | Ricerca | PARTIAL | `web.search/fetch/browse`, agente research. Mancano sessioni di ricerca persistenti, valutazione di affidabilità, esportazione. |
| 13 | Browser | PARTIAL | `web.browse/form`, `browser.rs`. Mancano schede, profili, monitoraggio siti. |
| 14 | Risorse digitali | PARTIAL → **nuovo** | `fs.search/list/move/delete` + `fs.usage` (spazio, file grandi, duplicati, temporanei). Mancano tag, ricerca semantica, cartelle smart. |
| 15 | Background | PARTIAL | Scheduler, automazioni, osservazioni di file (`/api/watches`). Menu bar, avvio automatico, batteria/rete: da verificare. |
| 16 | Avatar | PARTIAL → **migliorato** | Inattivo, ascolto, elaborazione, parla, errore, completamento, movimento ridotto, + attesa di autorizzazione e offline (ADR-0188). Mancano ricerca, esecuzione e collaborazione: serve un segnale reale dal Core. |
| 17 | Centro di comando | PARTIAL → **migliorato** | Plancia e sezioni esistenti; tavolozza ⌘K «Vai a…» (ADR-0194). Ricerca globale su ricordi, chat, automazioni e strumenti (ADR-0204). Mancano ricerca nei documenti e azioni rapide. |
| 18 | Conoscenza personale | PARTIAL | Memoria cognitiva, note, export/import. Mancano collezioni, riferimenti collegati, freschezza. |
| 19 | Privacy | PARTIAL → **nuovo** (inventario) | `privacy::redact`, audit, permessi, export della memoria. `/api/privacy/overview` (ADR-0187): inventario locale, provider cloud con chiave, strumenti per effetto, autonomia, connettori, MCP, osservazioni. Vista grafica «Privacy» in macOS (ADR-0204). Mancano cronologia dei permessi e controlli di conservazione. |
| 20 | Risorse | PARTIAL → **nuovo** | Profili Eco/Bilanciato/Prestazioni/Massimo (`/api/performance`, ADR-0186): regolano passi dell'agente, sotto-agenti e frequenza delle metriche. Mancano limiti CPU/RAM, scaricamento modelli, modalità batteria e banda: non applicabili senza misure dell'host. |
| 21 | Integrazioni | IMPLEMENTED/PARTIAL | Registro connettori, OAuth, plugin, client MCP con salute e politica (ADR-0182). Webhook: `/api/hooks`. |
| 22 | Emergenze | PARTIAL | `/api/emergency`, diagnosi dei piani. Il ciclo rilevamento→registrazione per gli incidenti elencati non è completo. |
| 23 | Qualità/prestazioni | PARTIAL | `/api/metrics`. Nessun confronto prima/dopo registrato: non inventato. |

## Fatto in questa sessione
- `fs.usage` (modulo `assets`): sola lettura, test su duplicati, file grandi, temporanei, percorsi esterni e collegamenti.
- Profili di workspace (modulo `workspaces`): CRUD del proprietario, piano di attivazione senza esecuzione, rifiuto di comandi e percorsi esterni.

## Prossimi interventi consigliati (per priorità)
1. ~~Profili di risorse (§20)~~ fatto (ADR-0186).
2. Stati dell'avatar mancanti e movimento ridotto (§16): nessuna dipendenza esterna.
3. Pannello Privacy (§19) sui dati già esistenti.
4. Indice incrementale del codice e grafo delle dipendenze (§8).
5. Mouse/accessibilità (§3) e visione (§4): richiedono permessi di sistema e un modello con visione; non verificabili in questo ambiente Linux.
