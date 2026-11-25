# Rapporto di validazione

Esiti misurabili dell'evoluzione cognitiva (vedi [GAP_ANALYSIS.md](GAP_ANALYSIS.md)). Aggiornato il 2026-11-25.

## Misure di comprensione

| Misura | Insieme | Esito | Note |
| --- | --- | --- | --- |
| Instradamento verso gli agenti | 65 frasi sintetiche | 65/65 | regressione (ADR-0171) |
| Instradamento verso gli agenti | 111 frasi del proprietario | 111/111 | **sviluppo**: le radici di parola sono state scritte guardando queste frasi, quindi il dato sovrastima |
| Tipo di enunciato | sviluppo, 39 frasi | 39/39 | regole regolate su queste |
| Tipo di enunciato | **validazione, 46 frasi** | **40/46 (87%)** | frasi scritte dopo le regole; **non scritte dal proprietario**, quindi indipendenza solo parziale. Errori noti: «il server non risponde più», «sembra che il disco sia pieno», «ho cancellato per sbaglio la cartella», «ancora lento» (osservazioni lette come istruzioni), «meglio di no» (annullamento), «va bene così, grazie» (cortesia letta come assenso) |

Gli errori noti **non sono stati corretti** guardando la validazione; se si correggono, le frasi passano nello sviluppo e se ne
scrivono di nuove. Per una misura vera servono frasi nuove del proprietario.

## Nuove capacità: prove eseguite (ADR-0173–0180)

| Capacità | Prove |
| --- | --- |
| Intento e contesto | 8 test unitari, 2 d'integrazione (piani reali, annullamento), dataset di sviluppo e validazione |
| Livelli di autonomia | 4 unitari (minimo fra livelli, nessun allentamento, validazione, livello per dispositivo), 2 d'integrazione (livelli 0 e 1 con la chat, missioni ferme sotto il 4, nessuna modifica da chat) |
| Stato del mondo | 7 unitari, 1 d'integrazione con le fonti reali |
| Pausa, vincoli, prova a secco | 1 unitario, 3 d'integrazione (pausa e ripresa in sola lettura, validazione, prova senza effetti anche da chat) |
| Collaborazione fra specialisti | 1 d'integrazione (sistema e memoria in ordine, ricordo davvero salvato, nessuna collaborazione con un solo specialista) |
| Memoria cognitiva | 4 unitari, 1 d'integrazione (progetto fuori contesto, affidabilità, consolidamento con audit) |
| Autocorrezione | 4 unitari, 2 d'integrazione (nessun nuovo tentativo su risorsa assente, diagnosi, scrittura verificata, contraddizioni) |
| Passaggio fra dispositivi | 3 unitari, 1 d'integrazione (ripresa unica, audit) |
| Prova HTTP completa | `tests/e2e/api_smoke.cjs` con controlli sulle nuove rotte (eseguito in CI) |

**Non provato:** passaggio con dispositivi reali e nodo vero; collaborazione con un modello cloud; sincronizzazione
periodica dello stato del mondo (oggi su richiesta); interfacce grafiche per tutte le nuove funzioni (si usano API e chat).

## Controlli eseguiti sul codice

Vedi il checkpoint in [ROADMAP.md](ROADMAP.md): `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`,
`cargo test --locked`, test Python e dei contratti.

## Non verificato

Percorso con modello cloud reale (nessuna chiave); aspetto nelle app; risoluzione di riferimenti con applicazione o
finestra attiva (il contesto automatico non le include); comportamento del modello sull'indicazione del Core.
