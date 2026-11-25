# Integrazione MCP

> Fonte unica per l'uso del protocollo MCP in JARVIS. Il catalogo degli strumenti è in [TOOL_SYSTEM.md](TOOL_SYSTEM.md);
> le decisioni sono in `DECISIONS.md` (ADR-0138 server, ADR-0156 client, ADR-0166 sessioni e Hugging Face, ADR-0182 robustezza e politica per strumento).

## Due direzioni

| Direzione | Cosa fa | Dove |
|---|---|---|
| **Server** | Espone ad altri agenti strumenti di calcolo e di sola lettura, sotto policy e audit | `POST /api/mcp` (solo proprietario) |
| **Client** | JARVIS usa gli strumenti di server MCP **che hai autorizzato tu** | `mcp.list`, `mcp.call`, `/api/mcp/servers` |

## Client: regole di sicurezza

- Solo il proprietario autorizza un server (`POST /api/mcp/servers`: `name`, `url`, `token` facoltativo). Né un modello né un piano
  possono aggiungerne. Il token va nel deposito dei segreti (`mcp:<nome>`) e non viene mai restituito.
- Solo HTTPS (HTTP solo su loopback), niente credenziali nell'indirizzo, niente reindirizzamenti, risposta massima 256 KiB, timeout 20 s,
  fino a 8 server.
- `mcp.list` ha rischio medio; **`mcp.call` chiede sempre la conferma**. Il risultato è marcato `untrusted`, passa dalla guardia
  anti-iniezione ed è un dato, mai un'istruzione.
- Il protocollo usato è Streamable HTTP: `initialize`, notifica `initialized` e identificativo di sessione (`Mcp-Session-Id`, solo
  ASCII visibile, massimo 200 caratteri) riportato nelle richieste successive. `Accept` include sia `application/json` sia
  `text/event-stream`; sono lette anche le risposte SSE **già concluse**. Flussi che restano aperti e server con OAuth non sono
  supportati.

## Robustezza e politica per strumento (ADR-0182)

- **Sessione riusata** per 10 minuti e riaperta da sola se il server la dimentica; elenco strumenti letto a pagine e in cache (5 minuti),
  con impronta: `changed: true` avvisa che uno strumento è comparso, sparito o ha cambiato schema.
- **Tentativi limitati:** HTTP 429/502/503/504 si ripetono (al più 3 tentativi, 40 s in tutto); un guasto di rete si ripete solo per le
  letture, **mai** per `mcp.call` (potrebbe aver avuto effetto).
- **Argomenti verificati** sullo schema del server (obbligatori, tipi, valori ammessi) prima di partire; immagini e audio restano segnaposto.
- **Salute e statistiche:** `POST /api/mcp/servers/health {"name": "…"}` (raggiungibilità, latenza, strumenti, strumenti di sola lettura);
  `GET /api/mcp/servers` mostra chiamate, errori, ultimo esito e latenza.
- **Politica per strumento** (`POST /api/mcp/servers/policy {"name": "…", "allow": […], "auto": […]}`, solo proprietario): `allow` limita gli
  strumenti chiamabili; `auto` elenca gli strumenti di **sola lettura** (dichiarati tali dal server, a elenco già letto) che JARVIS può usare
  **senza conferma** con `mcp.read`. `mcp.call` chiede sempre il consenso.

## Hugging Face

Il server pubblico `https://huggingface.co/mcp` permette di cercare modelli, dataset e Spaces e di leggere schede e documentazione
(strumenti osservati il 2026-11-25: `hf_whoami`, `hf_fs`, `hub_repo_search`, `hub_repo_details`, `dynamic_space`, tra gli altri: l'elenco
è del server e può cambiare; `mcp.list` mostra quello attuale).

1. (Facoltativo, consigliato) crea un token di **sola lettura** da <https://hf.co/settings/mcp>. Senza token il server funziona in
   forma anonima con limiti di richieste più bassi.
2. Autorizzalo dal tuo computer con il server noto (indirizzo del catalogo; `auto_suggested` approva anche gli strumenti di sola lettura
   consigliati `hub_repo_search`, `hub_repo_details`, `hf_fs` per l'uso senza conferma: omettilo per essere interpellato a ogni chiamata):

```bash
curl -s -X POST http://127.0.0.1:8765/api/mcp/servers \
  -H "Authorization: Bearer $JARVIS_TOKEN" -H 'Content-Type: application/json' \
  -d '{"preset": "huggingface", "token": "hf_...", "auto_suggested": true}'
```

   La competenza «Modelli e dataset su Hugging Face» guida le richieste («cerca un modello su Hugging Face per…»).

3. Prova `mcp.list` (server `huggingface`) e poi `mcp.call` con lo strumento voluto: JARVIS chiede la conferma a ogni chiamata.

**Riservatezza.** Gli argomenti di `mcp.call` partono verso Hugging Face: non metterci dati personali, ricordi o contenuti di file.
`hf_whoami` restituisce il tuo profilo Hugging Face (mai il valore del token). Il risultato è un dato, non cambia le regole di JARVIS.

**A cosa serve per migliorare JARVIS.** Trovare e confrontare modelli per le rotte cloud (codice, traduzione, analisi) e leggere le
schede dei modelli prima di decidere se aggiungere un provider; non sostituisce la scelta dei modelli, che resta tua.

## Non incluso

Provider di modelli Hugging Face dentro il Core, embeddings o riconoscimento vocale su servizi esterni (richiedono una regola di
riservatezza per i dati personali), server MCP con OAuth, flussi SSE a lunga durata.
