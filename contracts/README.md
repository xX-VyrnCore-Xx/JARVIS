# Contratti versionati JARVIS

Contratti condivisi fra Core Rust, AI Python, applicazioni e integrazioni (RNF-01). Formato: JSON Schema draft 2020-12. Ogni documento dichiara il campo `contract` (`jarvis.<nome>/<major>`); una modifica incompatibile crea una nuova major in `contracts/v<N>/`.

| Schema | Contratto | Fonte | Requisiti |
|---|---|---|---|
| `v1/common.schema.json` | Definizioni condivise: UUID, timestamp UTC, semver, identificativi, attori | ARCHITECTURE.md | RNF-01 |
| `v1/envelope.schema.json` | `jarvis.envelope/1` — messaggio fra moduli e agenti con correlazione | AI_SYSTEM.md, DEVOPS_QA_OBSERVABILITY.md | RF-03, RNF-07 |
| `v1/error.schema.json` | Errore propagato con categoria, ripetibilità ed effetto osservato | AUTONOMY_AND_TOOLS.md | RF-02, RNF-02 |
| `v1/delegation.schema.json` | `jarvis.delegation/1` — delega temporanea a un agente | AI_SYSTEM.md | RF-03 |
| `v1/tool-descriptor.schema.json` | `jarvis.tool/1` — descrittore di strumento con effetti, rischio, retry e conferma | AUTONOMY_AND_TOOLS.md, SECURITY_AND_PRIVACY.md | RF-05, RNF-04 |

## Regole incluse negli schemi

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

## Verifica

```bash
python3 -m pip install -r tests/contracts/requirements.txt
python3 -m unittest discover -s tests/contracts -v
```

Le fixture in `tests/contracts/fixtures/` sono sintetiche e non contengono dati personali.
