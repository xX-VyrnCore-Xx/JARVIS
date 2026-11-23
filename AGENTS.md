# JARVIS

Consulta [CLAUDE.md](CLAUDE.md) per il flusso di lavoro del progetto.

## Scorrimento dell’interfaccia

In ogni modifica grafica mantieni l’intestazione e i controlli a fondo finestra aderenti al viewport. Lo scorrimento deve avvenire nei pannelli interni. Verifica entrambi i limiti dello scroll su desktop e in una finestra stretta, senza spostamenti della finestra o vuoti sopra e sotto l’app. Non esiste una verifica automatica dello scorrimento nel client nativo (`tests/e2e/ui_smoke.cjs` è stato ritirato con l'interfaccia Web): eseguila a mano su macOS e Windows e registra l'esito nel checkpoint.

## Privacy nella documentazione

Usa dati sintetici e domini riservati agli esempi. Escludi credenziali, identificativi privati, percorsi personali, riferimenti a conversazioni private e dettagli di deployment reali. Documenta i nomi delle impostazioni necessari all’uso e conserva i valori negli archivi dedicati.
