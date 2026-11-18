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
| Backup | XChaCha20-Poly1305 con chiave Argon2id dalla passphrase, nessuna chiave salvata, ripristino verificato prima della sostituzione | `core/server/tests/backup.rs` |
| Cartella dati | Blocco esclusivo: un solo Core per volta, nessun ripristino a Core attivo | `short_passphrase_is_refused_and_data_dir_is_locked` |
| Firma dell'audit (ADR-0030) | Ogni evento firmato con Ed25519, chiave privata nell'archivio dei segreti, verifica in `GET /api/audit`, chiave pubblica esportabile | `signatures_detect_a_recomputed_chain` |
| OAuth dei connettori (ADR-0027) | PKCE S256, `state` monouso di 10 minuti, callback solo da questo computer, token e refresh token cifrati, rinnovo automatico, revoca locale | `oauth_pkce_flow_authorizes_refreshes_and_revokes` |
| Notifiche esterne (ADR-0026) | Canali ntfy e Telegram scelti dal proprietario; per impostazione solo titolo e categoria; token nell'archivio dei segreti, mai restituiti; HTTPS obbligatorio salvo loopback; invio fuori dal blocco del Core; audit di aggiunta e rimozione | `external_channels_receive_filtered_private_notifications` |
| Segreti dei connettori | Cifrati per valore (XChaCha20-Poly1305), chiave nel portachiavi del sistema su macOS e Windows, in `data/.secrets.key` con permessi 0600 su Linux (ADR-0021); mai restituiti da API, audit, log o agente | `install_authorize_call_and_secret_never_leaks`, `key_moves_from_file_to_keychain_and_secrets_stay_readable` |
| Connettori | Host dichiarati e policy SSRF, nessun redirect; strumenti presenti solo se autorizzati; scritture esterne sempre con conferma; revoca immediata | `writes_need_confirmation_and_revoke_denies_immediately` |
| Webhook | HMAC-SHA256 del corpo con segreto mostrato una volta; solo notifiche, nessuna azione; firme non valide nell'audit | `webhooks_require_a_valid_signature` |
| Dispositivi remoti | Solo host ammessi da `JARVIS_ALLOWED_HOSTS` dietro un proxy HTTPS fidato; pagina senza token di sessione; codice monouso (40 bit, 10 minuti, blocco dopo 5 errori); token conservato come SHA-256; permessi `chat` e `confirm`; revoca immediata | `core/server/tests/devices.rs` |
| Dipendenze | `cargo audit` e `pip-audit` in CI: una vulnerabilità nota blocca la modifica (SQLite aggiornata per RUSTSEC-2026-0459) | job «audit delle dipendenze» |
| Integrità della release | Archivi con impronta SHA-256, bozza di release approvata dal proprietario; firma dell'autore da aggiungere | `release.yml` |
| Nodi esecutori | Solo strumenti dichiarati all'associazione; policy, conferma e audit del Core; workspace del nodo verificata sia dal Core sia dal nodo; token come SHA-256 nell'hub e in `node.json` con permessi 0600; revoca con assegnazioni annullate e agente fermato | `core/server/tests/nodes.rs` |
| Dati in uscita (Web) | Interruttore `JARVIS_WEB`, domini consentiti e vietati, audit di ogni chiamata | `web_can_be_disabled_or_restricted_to_an_allowlist` |

Da completare:
- identità con passkey o WebAuthn e MFA;
- identità dei nodi e mTLS;
- isolamento dei plugin con codice;
- chiave dei segreti nel portachiavi del sistema operativo;
- cifratura dei dati a riposo e dei backup;
- firma o ancoraggio esterno dell'audit.
