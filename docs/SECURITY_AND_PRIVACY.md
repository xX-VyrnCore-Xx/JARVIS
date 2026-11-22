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
| Segreti dei connettori | Cifrati per valore (XChaCha20-Poly1305), chiave nel portachiavi del sistema su macOS, Windows e Linux con sessione grafica (ADR-0021, ADR-0033), altrimenti in `data/.secrets.key` con permessi 0600; mai restituiti da API, audit, log o agente | `install_authorize_call_and_secret_never_leaks`, `key_moves_from_file_to_keychain_and_secrets_stay_readable`, `linux_without_secret_service_falls_back_to_the_file` |
| Connettori | Host dichiarati e policy SSRF, nessun redirect; strumenti presenti solo se autorizzati; scritture esterne sempre con conferma; revoca immediata | `writes_need_confirmation_and_revoke_denies_immediately` |
| Webhook | HMAC-SHA256 del corpo con segreto mostrato una volta; solo notifiche, nessuna azione; firme non valide nell'audit | `webhooks_require_a_valid_signature` |
| Dispositivi remoti | Solo host ammessi da `JARVIS_ALLOWED_HOSTS` dietro un proxy HTTPS fidato; pagina senza token di sessione; codice monouso (40 bit, 10 minuti, blocco dopo 5 errori); token conservato come SHA-256; permessi `chat` e `confirm`; revoca immediata | `core/server/tests/devices.rs` |
| Dipendenze | `cargo audit` e `pip-audit` in CI: una vulnerabilità nota blocca la modifica (SQLite aggiornata per RUSTSEC-2026-0459) | job «audit delle dipendenze» |
| Integrità della release | SHA-256, attestazione GitHub/Sigstore; APK Android firmato con keystore persistente, installer desktop firmati con i certificati configurati nei Secret | `release.yml` |
| Nodi esecutori | Solo strumenti dichiarati all'associazione; policy, conferma e audit del Core; workspace del nodo verificata sia dal Core sia dal nodo; token come SHA-256 nell'hub e in `node.json` con permessi 0600; revoca con assegnazioni annullate e agente fermato | `core/server/tests/nodes.rs` |
| Dati in uscita (Web) | Interruttore `JARVIS_WEB`, domini consentiti e vietati, audit di ogni chiamata | `web_can_be_disabled_or_restricted_to_an_allowlist` |
| Dipendenze Python della voce | Versioni transitive e hash SHA-256 nel lock `ai/requirements-voice.txt`; installazione con `pip --require-hashes` | `web_can_be_disabled_or_restricted_to_an_allowlist` |
| Azioni CI | Riferimenti a commit SHA immutabili; CLI Tauri installata con versione esatta | `web_can_be_disabled_or_restricted_to_an_allowlist` |

## Lacune e limiti residui

- **Identità del proprietario:** WebAuthn protegge le approvazioni dei dispositivi associati quando viene registrata una passkey; non costituisce MFA generale per ogni accesso al Core. L'API resta protetta dalla sessione del Core e la sua esposizione remota richiede il proxy HTTPS privato previsto.
- **Identità dei nodi:** mTLS è disponibile per gli esecutori quando `JARVIS_NODE_TLS_ADDR` è configurato; non va interpretato come requisito per tutti i trasporti interni o per i client.
- **Plugin con codice:** non sono supportati. I connettori installabili sono manifest dichiarativi e non eseguono codice plugin; un futuro runtime plugin richiederà isolamento prima di essere abilitato.
- **Dati a riposo:** credenziali e backup sono protetti come descritto sopra, ma il database applicativo SQLite non è cifrato integralmente. Proteggerlo con account utente, permessi del filesystem e cifratura del disco del sistema operativo.
- **Audit:** gli eventi sono firmati con Ed25519, ma chiave e registro sono sullo stesso host. La verifica indipendente richiede di conservare la chiave pubblica fuori dal sistema; non esiste un ancoraggio remoto a prova di amministratore locale.
- **Terminale:** `shell.run` resta un'esecuzione di comandi arbitrari con i permessi dell'utente dopo consenso esplicito, non una sandbox. È disattivato per impostazione predefinita e vietato nei workflow e nei nodi; il filtro di comandi pericolosi non sostituisce un isolamento. Output e tempo sono limitati, ma non usare il terminale su input non fidato.
- **Container Docker (ADR-0090):** n8n resta disabilitato per default e limitato a nodi interni fissi, senza rete, codice utente, credenziali o task runner; il container gira non root con root filesystem in sola lettura, volume dati dedicato, cache temporanea e risorse limitate. La sandbox Python è effimera, senza rete o mount della workspace, non root, in sola lettura e con limiti di CPU, memoria, processi, input, output e tempo. Entrambe usano il daemon Docker locale: l'accesso a quel daemon resta equivalente a un controllo elevato sul computer host, quindi va protetto e mantenuto aggiornato.


**Terminale (ADR-0038).** Ogni comando di `shell.run` richiede la conferma del proprietario con il testo esatto del comando; nessuna variabile `JARVIS_*` passa al processo; comandi con `sudo`, `doas`, `pkexec`, `runas` o distruttivi noti sono negati dalla policy; stdout e stderr vengono drenati ma conservano al massimo 8.000 caratteri ciascuno; il processo ha timeout di 60 secondi. Il terminale non è ammesso in automazioni e nodi e si disattiva con `JARVIS_SHELL=off`. Non è isolamento di processo: l'utente deve esaminare il comando prima di confermarlo.


**mTLS dei nodi (ADR-0041).** Con `JARVIS_NODE_TLS_ADDR` il Core apre una porta TLS solo per i nodi: certificato client obbligatorio e registrato all'associazione, Core fissato per impronta dal nodo, token e certificato richiesti insieme. La chiave privata del nodo non lascia mai il nodo; la revoca vale subito.


**Passkey (ADR-0042).** Un dispositivo associato può registrare una passkey (WebAuthn, ES256, verifica dell'utente obbligatoria). Da quel momento ogni approvazione da quel dispositivo richiede un'asserzione legata alla singola conferma; il Core conserva solo la chiave pubblica, rifiuta contatori non crescenti e registra tutto nell'audit. Rifiutare un'azione resta sempre possibile.


**Browser (ADR-0043).** Profilo temporaneo cancellato a ogni uso, nessuna estensione o sincronizzazione, nessun cookie del proprietario. Tutte le richieste della pagina sono intercettate e verificate con la policy Web (indirizzi privati vietati anche dopo la risoluzione DNS). Nella conferma di `web.form` i valori dei campi sensibili (password, token, carte, IBAN) sono mascherati.
