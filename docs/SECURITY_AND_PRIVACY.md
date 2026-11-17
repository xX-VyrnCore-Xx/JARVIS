# Sicurezza e privacy

JARVIS adotta Zero Trust. Utente, dispositivi e servizi hanno identità verificabili. Il modello prevede passkeys/WebAuthn e MFA per l'utente, identità crittografiche per i nodi e comunicazione cifrata con mTLS fra componenti dove previsto. La VPN mesh fornisce connettività. Ogni punto di esecuzione verifica comunque identità e policy.

| Confine | Controllo |
|---|---|
| Utente e client | Autenticazione, sessione, privilegi granulari, revoca |
| Nodo e servizi | Enrollment, identità, rotazione, revoca e comunicazione cifrata |
| Agente, plugin e tool | Isolamento, risorse limitate, policy e rischio per operazione |
| Azioni critiche | Conferma legata a parametri, destinatario ed effetto atteso |
| Input da siti, file e plugin | Trattamento come dati e verifica dei comandi proposti |
| Cloud | Controllo di ammissibilità e minimizzazione prima dell'invio |
| Dati a riposo | Protezione di credenziali e backup, retention configurabile |
| Audit | Identità, decisione di policy, azione, correlazione ed esito |

Centralizzare i segreti e limitarne l'accesso ai componenti autorizzati. Tenere credenziali e payload personali fuori dal repository, dai checkpoint e dalla telemetria ordinaria. Un errore di validazione o una policy assente interrompono l'azione che richiede autorizzazione. Un dispositivo revocato perde accesso ai servizi e alle assegnazioni.

Per resistere alla prompt injection, mantenere distinti contenuto consultato, richieste del proprietario e decisione di policy. Verificare il confine anche per argomenti generati da un agente. Testare revoca durante un task, scadenza della conferma, tentativo di accesso oltre ambito, riuso di una conferma, divulgazione a un provider e manipolazione di input e log.

Documentare insieme ai contratti la classificazione dei dati, i flussi egress, la retention, i ruoli minimi e le procedure di revoca e ripristino.

## Controlli implementati nel prototipo v0.1

| Controllo | Implementazione | Verifica |
|---|---|---|
| Esposizione di rete | Ascolto solo su loopback; host non di loopback rifiutato all'avvio | `config.rs` |
| DNS rebinding e CSRF | Header `Host` in lista ammessa; token di sessione casuale richiesto da `/api/*` | `api_requires_token_and_local_host` |
| Iniezione di script | CSP `script-src 'self'`; testo inserito solo con `textContent` | E2E: nessun errore CSP |
| Ambito dei file | Policy lessicale, poi controllo canonico in esecuzione (link simbolici compresi) | `path_traversal_is_denied_by_policy`, `symlink_escape_is_blocked_at_execution` |
| Azioni critiche | Conferma monouso, con scadenza, legata al digest dei parametri | `high_risk_write_requires_single_use_confirmation` |
| Audit | JSONL append-only a catena SHA-256, verificato a ogni lettura | `audit_chain_records_every_decision` |
| Minimizzazione | Nessun backend cloud; fixture sintetiche; diagnostica senza payload | ADR-0004 |
| SSRF (Web) | Solo http e https pubblici; niente loopback, LAN o link-local (metadati cloud); DNS verificato e controllo ripetuto a ogni redirect | `ssrf_targets_are_denied_by_default`, `redirect_to_blocked_domain_is_stopped_at_each_hop` |
| Prompt injection (Web) | Contenuto marcato non attendibile; dopo un risultato di strumento sono accettate solo risposte testuali | `page_instructions_are_never_executed`, `test_web_tool_result_is_wrapped_as_untrusted_for_the_model` |
| Voce | Motori solo locali; audio non salvato né registrato nell'audit; parlato senza «Jarvis» scartato; microfono sempre segnalato; nessuna conferma a voce | `tests/ai/test_voice.py`, `core/server/tests/voice.rs`, E2E |
| Dati in uscita (Web) | Interruttore `JARVIS_WEB`, domini consentiti e vietati, audit di ogni chiamata | `web_can_be_disabled_or_restricted_to_an_allowlist` |

Da completare:
- identità con passkey o WebAuthn e MFA;
- identità dei nodi e mTLS;
- isolamento dei plugin;
- cifratura dei dati a riposo e dei backup;
- firma o ancoraggio esterno dell'audit.
