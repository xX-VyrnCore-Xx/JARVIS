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
