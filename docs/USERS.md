# Utenti e personalità

> Stato: **implementato** (ADR-0169). Fonte unica per gli account degli utenti; i permessi del proprietario sono in
> [SECURITY_AND_PRIVACY.md](SECURITY_AND_PRIVACY.md) e nell'ADR-0116.

## Idea

JARVIS ha un **proprietario** (chi installa il Core, con tutti i poteri) e può avere più **utenti**: persone con un account, una
conversazione, ricordi e una **personalità** proprie. Un utente **non ha strumenti** e non vede file, ricordi, conversazioni, segreti o
dispositivi del proprietario né di altri utenti: parla con il modello cloud che il proprietario decide di mettergli a disposizione.

## Chi può registrarsi

Il proprietario sceglie (pagina **Utenti** o `POST /api/users/settings`):

| `registration` | Effetto |
|---|---|
| `invite` (predefinito) | serve un **invito monouso** (`POST /api/users/invites`): 32 caratteri casuali, vale 72 ore, si vede una sola volta |
| `closed` | nessuna nuova registrazione |
| `open` | basta un nome utente e una password (sconsigliato se il Core è raggiungibile da Internet) |

`cloud` (predefinito **spento**): se attivo, gli utenti possono chattare con il modello cloud configurato dal proprietario. È una scelta
esplicita perché ha un costo e perché i messaggi degli utenti vanno a quel fornitore. Spento, restano solo i comandi personali.

## Accesso

- **Registrazione** `POST /api/auth/register` (`username`, `password`, `display_name`, `invite`, `personality` facoltativa) e
  **accesso** `POST /api/auth/login` restituiscono un **token di sessione** (64 caratteri, valido 30 giorni, revocabile).
  Il token si invia nell'intestazione `x-jarvis-user`; è diverso dal token del proprietario e dai token dei dispositivi e **non apre nulla
  fuori da `/api/me*` e `/api/auth/*`**.
- Password: da 10 a 128 caratteri, senza il nome utente e non fra quelle più comuni; si conserva solo l'impronta **Argon2id** (19 MiB, 2
  passaggi) con salt casuale. Nel database restano soltanto l'impronta della password e l'hash dei token e degli inviti.
- Protezioni: dopo **5 accessi falliti** per un nome utente l'accesso si ferma per 15 minuti (un accesso riuscito azzera il conto), c'è un
  limite complessivo sui tentativi, le risposte per nome inesistente, password sbagliata e account disattivato sono **identiche** e costano
  lo stesso tempo, e i calcoli di Argon2 contemporanei sono limitati (non si può esaurire la memoria con richieste anonime).

## La personalità dell'utente

Ogni utente ha due schede, salvate in una cartella privata solo sua (`users/<id>/`):

- **Come ti parlo** (`persona`): stile (maggiordomo digitale o sobrio), forma con cui chiamarti, frequenza, «tu» o «lei».
- **Chi sei** (`profile`): ruolo, settore, competenze, interessi, obiettivi, lingua, tono e livello tecnico. Con `share_with_cloud: false`
  non lasciano il Core.

Si creano alla registrazione (per nome, in modo informale), si rifiniscono con `PUT /api/me/personality` e **si imparano dalla chat** con
frasi di tutti i giorni, senza chiamare il modello: «chiamami Marco», «dammi del tu / del lei», «sii più diretto / cordiale / formale».
`GET /api/me` restituisce anche un **riepilogo leggibile** («Ti chiamo Marco · Ti do del tu · Interessi: vela»). La personalità del
proprietario non cambia mai.

## Memoria e cronologia

«**Ricorda che** …» salva un ricordo (fino a 100), «**dimentica tutto**» li cancella; i ricordi si vedono e si eliminano da
`/api/me/memory`. La conversazione (ultimi 400 messaggi) si vede e si cancella da `/api/me/history`. Messaggi e ricordi sono **cifrati
con la chiave della chat del Core**. `DELETE /api/me` elimina l'account con tutti i suoi dati.

## Che cosa vede il proprietario

Elenco di account (nome, data, ultimo accesso, numero di messaggi e sessioni): **mai** il testo dei messaggi o dei ricordi dall'interfaccia.
Può creare inviti, **disattivare** (chiude le sessioni) o riattivare un account ed **eliminarlo** con i suoi dati. Resta vero che chi ha
accesso al computer e ai file del Core tecnicamente controlla anche la chiave di cifratura: la separazione è fra i **profili**, non una
protezione dall'amministratore del sistema.

## Come si usa dalle app

- **Utente:** nel client egui (Windows e macOS) apri «Collega JARVIS», scegli *Registrati* (con l'invito) o *Accedi*, indica l'indirizzo
  del Core. Vedi solo **Chat** e **Profilo**; la sessione si ricorda finché non esci.
- **Proprietario:** pagina **Utenti** (client egui e console SwiftUI) per registrazioni, uso del modello cloud, inviti e account.
  La console SwiftUI si collega solo al Core locale: l'accesso utente è nel client egui.

## Limiti

Gli utenti non possono usare strumenti, file, connettori, automazioni o piani; la chat dell'utente non ha streaming; non esistono
reimpostazione della password via email né accesso con provider esterni (se si dimentica la password il proprietario elimina l'account e
invia un nuovo invito).
