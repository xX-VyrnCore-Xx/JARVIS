# Stato di implementazione

Fonte autorevole dello stato di ogni requisito dell'evoluzione «piattaforma di intelligenza artificiale personale».
Aggiornata il 2026-11-25 sul codice di `main` più la PR in corso (ADR fino alla 0165). Legenda: **Implementato** (funziona end-to-end ed è
verificato da test), **Parziale** (funziona in parte: il limite è indicato), **Sperimentale** (c'è, non maturo),
**Pianificato** (assente). Nessuna voce è un segnaposto: ciò che non esiste è dichiarato «Pianificato».
Verifiche ed esiti per revisione: [ROADMAP.md](ROADMAP.md). Piano dei passi successivi: [IMPLEMENTATION_ROADMAP.md](IMPLEMENTATION_ROADMAP.md).

## 1. Sistema degli strumenti ([TOOL_SYSTEM.md](TOOL_SYSTEM.md))

| Componente | Stato | Note |
| --- | --- | --- |
| Registry universale | Implementato | 78 descrittori; `GET /api/tools/registry` con filtri; categoria, piattaforme, offline, reversibilità, dipendenze e hardware derivati (sovrascrivibili) |
| Discovery | Parziale | Catalogo incorporato, connettori, n8n e pacchetti installati; nessuna scoperta di plugin dal disco |
| Execution engine | Implementato | validazione, policy, conferma, esecuzione, audit |
| Selection engine | Parziale | Instradamento agenti per parole chiave e competenze; nessuna selezione appresa |
| Permission manager | Implementato | policy, profili e regole del proprietario (ADR-0116), arresto di emergenza |
| Validation engine | Parziale | input validato a ogni chiamata; nei piani anche le condizioni di successo sul risultato (`expect`); l'output non è validato contro `output_schema` |
| Dependency manager | Parziale | dipendenze dichiarate; stato di Docker; nessuna installazione automatica |
| Lifecycle manager | Parziale | blocco/sblocco e conferma per regola; nessuna installazione o rimozione di strumenti nativi |
| Health monitor / History | Implementato | `GET /api/tools/health`, `/history` (ADR-0151) |
| Versioning | Parziale | versione esatta richiesta dalla policy; nessuna coesistenza di più versioni |
| Chaining / Composition | Implementato | piani con riferimenti fra passi, condizioni di successo, modelli di piano (ADR-0152, 0154, 0159) e workflow |
| Tool SDK | Parziale | SDK dichiarativo per competenze e modelli di piano (ADR-0165, [PLUGIN_SYSTEM.md](PLUGIN_SYSTEM.md)); per nuovi strumenti resta la procedura in TOOL_SYSTEM.md |
| Plugin manager | Parziale | pacchetti dichiarativi (competenze e procedure) con installazione atomica e rimozione; connettori con manifest e n8n; nessun plugin nativo in codice |
| MCP | Implementato | server `/api/mcp` (calcolo e sola lettura) e **client** con server autorizzati dal proprietario, conferma sempre richiesta e risultati non attendibili (ADR-0156); manca lo streaming SSE |

## 2. Strumenti operativi

**Sistema e computer.** File system, terminale (con conferma) e sandbox, controllo del desktop (conferma), monitoraggio
hardware: **Implementato**. Gestione applicazioni (apri, porta in primo piano, tasti, testo, finestre) e processi (solo elenco),
spazio su disco, rete (`it.subnet`, solo calcolo), schermata (OCR locale del testo, nessuna immagine salvata), backup
cifrato con verifica: **Parziale**. Servizi, configurazione di sistema, pacchetti, appunti: **Pianificato**.

**Sviluppo e infrastruttura.** Esecuzione di codice (sandbox Docker senza rete, profili di risorse): **Implementato**.
Analisi del codice (sintassi Python, JSON), generazione di codice (guidata dal modello e dalle competenze di sviluppo),
GitHub e GitLab (connettori), Docker (stato e sandbox; nessuna gestione dei container), nodi remoti, debug e test (competenze +
sandbox), audit di sicurezza (`security.*` + competenza), integrazione API (connettori): **Parziale**. Git (stato, cronologia, differenze, rami in sola lettura; commit con conferma), analisi dei log e
dipendenze (lettura dei manifest con segnalazione di quelle non fissate): **Implementato**. SSH, virtualizzazione,
database, build, deploy, CI/CD: **Pianificato** (oggi si usano attraverso il terminale con conferma o la sandbox).

**Web e ricerca.** Ricerca, pagina con browser reale, compilazione di moduli (con conferma), estrazione: **Implementato**.
Ricerca con più fonti e confronto (competenza «ricerca»), documentazione: **Parziale**. Download, monitoraggio di siti:
**Pianificato**.

**Produttività.** Cose da fare, note, promemoria, calcolo, analisi dei dati (`data.stats`, `biz.*`), notifiche: **Implementato**.
Riepilogo di tabelle CSV (`table.summary`), lettura di fogli XLSX (`sheet.read`), testo dei PDF (`pdf.extract`, solo PDF con testo) e appunti di sistema (`clipboard.read`/`write`, sempre con conferma): **Implementato** (ADR-0162). Posta e calendario (Gmail, Outlook, Google Calendar tramite connettori; richiedono la configurazione OAuth del proprietario),
progetti (missioni, piani, cose da fare): **Parziale**. Scrittura di PDF e XLSX, OCR, presentazioni, contatti: **Pianificato**.

**IA e multimodale.** Memoria semantica, episodica e procedurale con grafo, recupero, riconoscimento e sintesi vocale (voce
maschile), OCR locale, delega agli agenti, scelta del modello: **Implementato**. Pianificazione (piani scritti dal
proprietario o dall'assistente): **Parziale**. Visione, analisi di immagini, generazione di contenuti multimediali, costruttore
dinamico di strumenti: **Pianificato**.

## 3. Motore autonomo ([AUTONOMOUS_ENGINE.md](AUTONOMOUS_ENGINE.md))

| Capacità | Stato | Note |
| --- | --- | --- |
| Grafo delle dipendenze, checkpoint, persistenza, ripresa | Implementato | piani cifrati nel database; ripresa dopo la chiusura |
| Annullamento, budget di passi e di tempo, timeout, ripetizioni | Implementato | |
| Human-in-the-loop, rollback, replay, idempotenza | Implementato | |
| Esecuzione parallela dei passi | Implementato | fino a 4 passi indipendenti e senza stato insieme; i passi con stato o conferma restano in sequenza (ADR-0154) |
| Scomposizione automatica dell'obiettivo, ripianificazione | Parziale | missioni e agenti guidati dal modello; modelli di piano per le procedure ricorrenti (ADR-0159); **il modello cloud propone una bozza di piano che il Core valida** (ADR-0160); dopo un errore `POST /api/plans/{id}/replan` propone un piano di recupero, sempre come bozza (ADR-0161); l'esecuzione automatica del recupero non c'è |
| Validazione dei risultati | Implementato | condizioni `expect` per passo; manca un controllo indipendente guidato da un secondo modello |
| Ciclo di 14 fasi dichiarato | Parziale | contesto, vincoli, permessi, esecuzione, consegna e memoria esistono; pianificazione e validazione non sono ancora un unico ciclo |

## 4. Intelligenza avanzata

| Funzione | Stato | Note |
| --- | --- | --- |
| Proattività | Implementato | briefing quotidiano e settimanale, disco, conferme, automazioni in errore, cose lasciate indietro (ADR-0097, 0142) |
| Consapevolezza del contesto | Parziale | profilo, ora del giorno, memoria, chat; non il contesto applicativo/di progetto |
| Memoria persistente | Implementato | semantica, episodica, procedurale, ricordi fissati, scadenze, modifica, cancellazione |
| Apprendimento adattivo | Parziale | preferenze, valutazioni delle risposte per modello e **procedure salvate dal proprietario** come modelli di piano (ADR-0159); nulla viene appreso da solo |
| Auto-riparazione | Parziale | ripiego di modello, pulizia dei container rimasti, notifiche di errore, piani interrotti riprendibili |
| Auto-valutazione | Parziale | valutazioni e profili di risposta; nessun controllo indipendente dei risultati |
| Assistenza predittiva | Parziale | suggerimenti dei briefing; senza azioni non autorizzate |
| Automazione in linguaggio naturale | Parziale | «ogni giorno alle 08:00 …» offline e `workflow.create`; grammatica limitata |
| Automazione guidata da eventi | Implementato | calendario, webhook dei connettori e **modifiche di file e cartelle** della workspace (ADR-0158); non variazioni di risorse di sistema |
| Digital Environment Twin | Implementato | `GET /api/twin`: dispositivi, nodi, servizi, connettori, progetti e capacità, senza segreti (ADR-0157); aggiornato a ogni richiesta, non in modo continuo |
| Auto-miglioramento controllato | Implementato (limitato per scelta) | suggerimenti da evidenze locali; l'unica azione è salvare una procedura approvata dal proprietario; nessuna modifica di codice, sicurezza o permessi (ADR-0163) |

## 5. Multi-agente ([AUTONOMOUS_ENGINE.md](AUTONOMOUS_ENGINE.md))

Registro, scoperta e mappatura delle competenze (`GET /api/agents`), spiegazione dell'instradamento, delega con contesto
isolato e ambito di strumenti concesso, protocollo di delega a contratto: **Implementato**. Valutazione delle prestazioni per agente (lavori, successo, durata): **Implementato** (ADR-0157). Contesto condiviso e collaborazione
fra agenti: **Parziale** (delega a un agente alla volta). Supervisione (pausa e quota oraria per agente, decise dal proprietario, ADR-0164) e conflitti (decide il generalista): **Implementato**. Priorità fra agenti in parallelo e quote di tempo o memoria: **Pianificato** (esistono limiti di passi per agente).

## 6. IA ibrida e instradamento ([AI_SYSTEM.md](AI_SYSTEM.md))

Sette rotte per compito (veloce, ragionamento, codice, contesto lungo, scrittura, traduzione, analisi), ripiego su modello
alternativo, blocco delle credenziali, profilo e contesto condivisi solo se consentiti, funzionamento offline parziale a regole,
spiegazione dell'instradamento (`GET /api/models/route`): **Implementato**. **Modelli locali (Ollama in locale)**, scelta in
base a hardware, costi o capacità multimodali: **Pianificato**. I provider sono Ollama Cloud e NVIDIA NIM (ADR-0023), non
servizi obbligatori: senza chiave si lavora offline.

## 7. Sicurezza e permessi ([SECURITY_AND_PRIVACY.md](SECURITY_AND_PRIVACY.md))

Minimo privilegio e deny-by-default, rischio LOW/MEDIUM/HIGH/CRITICAL e conferme, ambiti di capacità per agente e per dispositivo,
sandbox Docker, deposito delle credenziali nel portachiavi, audit a catena con firma, cifratura a riposo di chat, attività,
notifiche e piani, guardia anti-iniezione sui risultati degli strumenti, validazione di input, revoca immediata dei
dispositivi, arresto di emergenza: **Implementato**. Ruoli: proprietario e dispositivi con ambiti (`chat`, `confirm`): **Parziale**
(nessun RBAC multiutente). Limitazione della frequenza (accessi remoti, richieste cloud) e quote (sandbox, budget dei piani):
**Parziale**.

## 8. Piattaforme

Windows e macOS: Core, app nativa (egui su Windows, SwiftUI su macOS), installatori: **Implementato**. Android: client con
associazione, chat, conferme, notifiche, attività: **Parziale** (nessuna nuova chat né ascolto vocale). Il Core headless gira
anche su Linux. Nodi remoti autorizzati con strumenti propri: **Implementato**. Le capacità non disponibili su una piattaforma
sono dichiarate nel registro (`platforms`), non simulate.

## 9. Interfaccia

Plancia, chat (multiple, in background), attività, **Piani** e **Strumenti** (nuovi), modelli, missioni, automazioni, memoria,
competenze, connettori, dispositivi, sicurezza (con arresto di emergenza), permessi, audit, sistema, aggiornamenti, notifiche,
profilo: **Implementato** su macOS e Windows (Android: chat, conferme, attività, notifiche). Gestione degli agenti (solo API),
editor visivo di workflow, grafo temporale unificato, monitor di infrastruttura dedicato: **Parziale** o **Pianificato**.

## 10. Qualità

Test unitari e di integrazione del Core, test Python degli agenti e dei contratti, smoke E2E dell'API, test dei client:
**Implementato** (comandi in [DEVELOPMENT.md](DEVELOPMENT.md); CI su Linux, macOS, Windows, Android). Prove con dispositivi reali,
Docker, Tailscale e microfono: **sospese** (non disponibili in CI). Test di prestazioni: `tests/perf`.
