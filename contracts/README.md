# Contratti versionati JARVIS

Contratti condivisi fra Core Rust, AI Python, applicazioni e integrazioni (RNF-01). Formato: JSON Schema draft 2020-12. Ogni documento dichiara il campo `contract` (`jarvis.<nome>/<major>`); una modifica incompatibile crea una nuova major in `contracts/v<N>/`.

| Schema | Contratto | Fonte | Requisiti |
|---|---|---|---|
| `v1/common.schema.json` | Definizioni condivise: UUID, timestamp UTC, semver, identificativi, attori | ARCHITECTURE.md | RNF-01 |
| `v1/envelope.schema.json` | `jarvis.envelope/1` — messaggio fra moduli e agenti con correlazione | AI_SYSTEM.md, DEVOPS_QA_OBSERVABILITY.md | RF-03, RNF-07 |
| `v1/error.schema.json` | Errore propagato con categoria, ripetibilità ed effetto osservato | AUTONOMY_AND_TOOLS.md | RF-02, RNF-02 |
| `v1/delegation.schema.json` | `jarvis.delegation/1` — delega temporanea a un agente | AI_SYSTEM.md | RF-03 |
| `v1/tool-descriptor.schema.json` | `jarvis.tool/1` — descrittore di strumento con effetti, rischio, retry e conferma | AUTONOMY_AND_TOOLS.md, SECURITY_AND_PRIVACY.md | RF-05, RNF-04 |
| `v1/task.schema.json` + `v1/task-transitions.json` | `jarvis.task/1` — task con obiettivo, condizioni, dipendenze e storia delle transizioni | AUTONOMY_AND_TOOLS.md, ARCHITECTURE.md | RF-02, RF-07 |
| `v1/policy-decision.schema.json` | `jarvis.policy-decision/1` — decisione del Security Engine con conferma vincolata | SECURITY_AND_PRIVACY.md | RNF-04, RNF-05 |
| `v1/audit-event.schema.json` | `jarvis.audit-event/1` — evento di audit in catena di hash | SECURITY_AND_PRIVACY.md, DEVOPS_QA_OBSERVABILITY.md | RNF-04, RNF-07 |
| `v1/agent-result.schema.json` | `jarvis.agent-result/1` — esito di un agente: risposta oppure una proposta di strumento, con backend e motivo di degradazione | AI_SYSTEM.md | RF-01, RF-04 |
| `v1/workflow.schema.json` | `jarvis.workflow/1` — automazione pianificata (intervallo, giornaliera, singola) con passi dal catalogo e approvazione del proprietario legata al digest dei passi | AUTONOMY_AND_TOOLS.md | RF-07 |
| `v1/workflow-run.schema.json` | `jarvis.workflow-run/1` — esecuzione con trigger, istanti, decisione ed esito per passo | AUTONOMY_AND_TOOLS.md | RF-07, RNF-07 |
| `v1/notification.schema.json` | `jarvis.notification/1` — notifica con gravità, categoria, sorgente, correlazione al task o al workflow, chiave di deduplicazione e collegamento a una vista interna | DEVOPS_QA_OBSERVABILITY.md | RF-13, RNF-07 |
| `catalog/tools/*.json` | Catalogo iniziale degli strumenti (`jarvis.tool/1`), con `input_schema` chiuso | AUTONOMY_AND_TOOLS.md | RF-05 |

La delega v1 accetta il campo facoltativo `inputs`: turni recenti (`user` o `assistant`), memoria autorizzata e `tool_result`. È un'aggiunta compatibile: i documenti senza `inputs` restano validi.

## Dipendenze fra moduli e contratti

| Contratto | Produttore (proprietario della scrittura) | Consumatori |
|---|---|---|
| `jarvis.task/1` | Core Runtime (registrazione), Orchestrator (transizioni) | Client tramite API Gateway, Automation, Observability |
| `jarvis.delegation/1` | Orchestrator | Multi-Agent Runtime, agenti |
| `jarvis.envelope/1` | Ogni modulo e agente | Destinatario indicato, Observability |
| `jarvis.tool/1` | Computer Use, Web Agent, Integration Engine e plugin | Orchestrator, agenti, Security Engine |
| `jarvis.policy-decision/1` | Security Engine | Orchestrator, esecutori degli strumenti, client per la conferma |
| `jarvis.audit-event/1` | Security Engine ed esecutori | Observability, verifiche di accettazione |
| `jarvis.agent-result/1` | Agenti (worker Python) | Orchestrator |
| `jarvis.workflow/1`, `jarvis.workflow-run/1` | Automation Engine (Core) | Interfaccia, Observability |
| `jarvis.notification/1` | Orchestrator, Security Engine, Automation Engine, monitor | Interfaccia (campanella, toast, notifiche del browser) |
| `error` (incorporato) | Ogni modulo | Orchestrator per retry e ripianificazione |

Ordine d'uso in un'esecuzione (ARCHITECTURE.md): il client crea un `task`; l'Orchestrator emette `delegation` in un `envelope`; l'agente propone uno strumento descritto da `tool`; il Security Engine produce `policy-decision` (eventualmente con conferma) e un `audit-event`; l'esecutore restituisce esito o `error` e un nuovo `audit-event`; l'Orchestrator aggiorna la `history` del task.

## Regole incluse negli schemi

- Un task `failed` contiene `error`.
- Una decisione `require_confirmation` contiene `confirmation` monouso; `allow` e `deny` non la contengono. Il rischio `high` o `critical` non porta mai a `allow`.
- Un evento di audit `allowed`, `denied` o `confirmed` riporta `decision_id`; un evento `failed` riporta `error_code` ed `effect`.

- Un messaggio con `status: failed` contiene `error`; gli altri stati non lo contengono.
- Un errore con effetto `unknown` o `partial` non è ripetibile: lo stato della risorsa va verificato prima.
- Errori `validation`, `unauthorized`, `forbidden` e `policy_denied` hanno `effect: not_applied` e `retryable: false`.
- Uno strumento con effetti di scrittura usa retry `never` o `idempotency_key`, mai `safe`.
- Uno strumento a rischio `high` o `critical` richiede `confirmation: required`.
- Un agente `temporary` dichiara `role` e `temporary: true`.

## Invarianti semantici

Non esprimibili in JSON Schema. Ogni implementazione (Rust, Python, TypeScript) deve applicarli; il riferimento eseguibile è `semantic_errors` in `tests/contracts/test_contracts_v1.py`.

- Delega: `expires_at` segue `issued_at`; `timeout_ms` non supera la durata della delega.
- Envelope: `sender` diverso da `recipient`; `causation_id` diverso da `message_id`.
- Task: la `history` inizia con `pending` senza `from`; ogni passo parte dallo stato precedente e segue `task-transitions.json`; gli stati terminali non hanno uscite; gli istanti non decrescono e non precedono `created_at`; `state` coincide con l'ultima transizione.
- Policy decision: `expires_at` segue `decided_at`; la conferma usa lo stesso `parameters_digest` della richiesta e scade dopo `decided_at` ed entro `expires_at`.
- Workflow run: `finished_at` non precede `started_at`; gli `index` dei passi sono 0, 1, 2, … senza buchi.
- Workflow (applicato dal Core a ogni esecuzione): `approval.steps_digest` = SHA-256 del JSON canonico di `steps`; se non coincide l'esecuzione fallisce con `security.approval_mismatch`.
- Audit: in un flusso `sequence` parte da 0 senza buchi; `prev_hash` del primo evento è 64 zeri, poi l'`hash` del precedente; `hash` = SHA-256 esadecimale del JSON canonico dell'evento senza `hash` (chiavi ordinate, separatori `,` e `:` senza spazi, UTF-8 senza escape dei caratteri non ASCII).

Implementazioni di riferimento: Python `tests/contracts/test_contracts_v1.py`, Rust `core/contracts` (crate `jarvis-contracts`). Entrambe usano le stesse fixture e devono dare gli stessi esiti.

## Verifica

```bash
python3 -m pip install -r tests/contracts/requirements.txt
python3 -m unittest discover -s tests/contracts -v
cd core && cargo test --locked
```

Le fixture in `tests/contracts/fixtures/` sono sintetiche e non contengono dati personali.
