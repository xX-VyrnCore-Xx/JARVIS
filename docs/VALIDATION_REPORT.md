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

## Controlli eseguiti sul codice

Vedi il checkpoint in [ROADMAP.md](ROADMAP.md): `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`,
`cargo test --locked`, test Python e dei contratti.

## Non verificato

Percorso con modello cloud reale (nessuna chiave); aspetto nelle app; risoluzione di riferimenti con applicazione o
finestra attiva (il contesto automatico non le include); comportamento del modello sull'indicazione del Core.
