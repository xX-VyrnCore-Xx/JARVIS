# Sistema degli strumenti

Fonte autorevole per registro, descrittori, esecuzione, salute, cronologia e ciclo di vita degli strumenti. Permessi e
policy: [SECURITY_AND_PRIVACY.md](SECURITY_AND_PRIVACY.md) e ADR-0116. Piani e agenti:
[AUTONOMOUS_ENGINE.md](AUTONOMOUS_ENGINE.md). Stato di ogni requisito: [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md).

## Registro universale

Gli strumenti nativi sono i descrittori JSON di `contracts/catalog/tools/` (schema `contracts/v1/tool-descriptor.schema.json`),
incorporati nel Core e validati all'avvio. Un descrittore dichiara: `name` (identificativo univoco, `famiglia.azione`),
`version`, `summary`, `input_schema` e `output_schema`, `scope`, `prerequisites`, `effects` (`none`, `read`,
`write_local`, `write_external`…), `risk` (`low`, `medium`, `high`, `critical`), `permissions`, `errors`, `timeout_ms`,
`retry` e `confirmation` (`none` o `required`).

`GET /api/tools/registry` restituisce per ogni strumento anche i campi normalizzati che i descrittori non dichiarano
(`src/toolsys.rs`): `category`, `platforms`, `offline`, `reversible` (`yes`, `no`, `n/a`), `dependencies` e `hardware`.
Un descrittore può sovrascrivere ciascuno scrivendolo esplicitamente. Filtri: `category`, `risk`, `platform`, `offline`, `q`.
Ogni voce porta la **salute** e la **regola del proprietario** (`owner_rule`: `deny` o `ask`).

| Campo del requisito | Dove |
| --- | --- |
| Identificativo, nome, descrizione, versione | descrittore |
| Categoria, piattaforme, offline, reversibilità, dipendenze, hardware | `toolsys.rs` (sovrascrivibili) |
| Schema di input e output | descrittore (l'input è validato a ogni chiamata) |
| Permessi, rischio, timeout, retry, errori | descrittore |

## Esecuzione e controlli

Ogni chiamata passa sempre dallo stesso percorso: validazione dell'input contro lo schema → `policy::decide` (strumento
noto, versione esatta, percorsi nella workspace, regole del proprietario, arresto di emergenza) → conferma del
proprietario se richiesta → esecuzione → audit. Né i modelli né gli agenti possono aggirarlo. I risultati degli strumenti
esterni (web, file, connettori) sono dati non attendibili e passano dalla guardia anti-iniezione (`guard.rs`, ADR-0149).

## Salute e cronologia

Ogni esecuzione registra strumento, istante, durata, esito e codice d'errore (mai argomenti né risultati) in `tool_runs`
(ultime 5000). `GET /api/tools/health` aggrega esecuzioni, errori, latenza media e massima, ultimo errore e stato:
`healthy` (errori < 10%), `degraded` (< 40%), `failing`, `unknown` (meno di tre esecuzioni). `GET /api/tools/history`
elenca le ultime esecuzioni (parametri `tool`, `limit`).

## Ciclo di vita

- **Blocco e sblocco**: una regola `deny` nel Centro permessi (`PATCH /api/permissions`) disattiva uno strumento o una
  famiglia (`web.*`); `ask` impone la conferma. Le regole restringono sempre, mai allargano.
- **Arresto di emergenza** (`POST /api/emergency`): nega ogni strumento che modifica o esegue, ferma lavori in background,
  piani, missioni e automazioni. Si disattiva solo dal computer del proprietario.
- **Dipendenze**: dichiarate (`docker`, `browser`, `n8n`…); lo stato di Docker è in `GET /api/docker`.
- **Versioni**: la policy accetta solo la versione esatta del catalogo; una nuova versione si pubblica con una nuova release.

## Estensioni: connettori, MCP e plugin

- **Connettori** (ADR-0047): manifest firmati dal proprietario per servizi esterni (Gmail, Calendar, Outlook, GitHub,
  GitLab, Slack, Todoist, Airtable, Open-Meteo), con credenziali nel secret store e le stesse policy.
- **MCP server**: `POST /api/mcp` espone ad altri agenti solo strumenti di calcolo e sola lettura (ADR-0138).
- **MCP client** (ADR-0156): `mcp.list` e `mcp.call` usano i server che il proprietario ha autorizzato in `/api/mcp/servers`
  (HTTPS, token nel deposito dei segreti). `mcp.call` chiede **sempre** la conferma; i risultati sono dati non attendibili.
- **n8n** (ADR-0093 e successivi): automazioni esterne gestite dal Core.
- Un plugin nativo caricato dal disco non esiste: l'estensione supportata è il connettore.

## Strumenti di sviluppo e dati

`git.status`, `git.log`, `git.diff`, `git.branch` (sola lettura) e `git.commit` (conferma; hook disattivati) usano il Git
installato, senza shell. `log.analyze` riassume un log, `deps.list` elenca le dipendenze dei manifest e segnala quelle non
fissate, `table.summary` riassume un CSV, `data.stats`, `text.extract` e `text.diff` analizzano numeri e testi, `time.wait`
distanzia i passi di un piano. Tutti lavorano dentro la workspace e non usano la rete (ADR-0150, 0155).

## Documenti e appunti

`pdf.extract` (testo di PDF con testo semplice), `sheet.read` (righe e riepilogo di fogli XLSX), `clipboard.read` e `clipboard.write`
(appunti, **sempre con conferma**). I contenuti letti da file o appunti sono marcati `untrusted`. Limiti e motivazioni nell'ADR-0162.

## SDK: aggiungere uno strumento

1. Scrivi `contracts/catalog/tools/<nome>.json` (copia un descrittore simile: effetti, rischio, schema, `timeout_ms`).
2. Registralo in `core/server/src/catalog.rs` e implementalo in `core/server/src/tools.rs` (funzione pura se possibile).
3. Se è di calcolo e sicuro, aggiungilo all'elenco MCP (`mcp_exposed`) e agli strumenti dei modelli cloud (`cloud::TOOLS`).
4. Scrivi test unitari e un test API; il test dei contratti verifica il descrittore.

Per estendere JARVIS senza scrivere codice nel Core esistono i **pacchetti di estensione** (competenze e modelli di piano): vedi [PLUGIN_SYSTEM.md](PLUGIN_SYSTEM.md). Strumenti nativi di terzi sono **pianificati**: richiedono esecuzione isolata e firma.
