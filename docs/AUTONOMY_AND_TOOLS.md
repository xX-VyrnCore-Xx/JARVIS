# Autonomia, strumenti e integrazioni

L'Orchestrator trasforma un obiettivo in un piano verificabile e aggiorna lo stato man mano che riceve gli esiti. Il perimetro comprende Computer Use, Web, produttività, sviluppo e amministrazione autorizzata, monitoraggio, workflow pianificati e integrazioni. Il Security Engine valuta ogni operazione sul dispositivo e sulla risorsa effettivi.

## Esecuzione

1. Registrare obiettivo, condizioni di successo, dipendenze, ambito e identità richiedente.
2. Convalidare gli argomenti dello strumento e valutare permessi, rischio e destinatario. Per le operazioni critiche presentare azione, destinazione ed effetto previsto prima della conferma.
3. Eseguire con risorse limitate e correlare il risultato al task. Registrare separatamente stato, effetto osservato ed eventuale errore.
4. Verificare la postcondizione, quindi completare, ripianificare o segnalare il blocco.

In caso di timeout con esito incerto, interrogare lo stato della risorsa prima di ripetere una scrittura. Usare un identificativo stabile per operazioni idempotenti e mantenere evidenze per ripresa dopo un arresto. L'annullamento interrompe il lavoro pendente e distingue gli effetti già applicati da quelli ancora in corso.

## Strumenti e workflow

Gli strumenti dichiarano input, output, ambito, prerequisiti, effetti, errori, timeout, modalità di retry e permessi. Ogni esecuzione programmata conserva istante, trigger e risultato. Un workflow conserva versione, dipendenze, operazioni e configurazione di frequenza. Applicare la policy vigente anche quando il workflow viene eseguito in background.

La decisione 81 prevede il framework di integrazione completo fin dall'inizio e un catalogo iniziale prioritario. Le categorie coprono servizi cloud e Web, produttività, sviluppo, database e infrastrutture, comunicazione, dispositivi, API, webhook e plugin. Le integrazioni concrete crescono per milestone. Il catalogo e l'ordine dei singoli connettori richiedono ancora conferma. I plugin operano con isolamento e permessi revocabili.

Nel prototipo il framework è l'Integration Engine dichiarativo (ADR-0013): un manifest `jarvis.connector/1` diventa un insieme di strumenti `connector.<id>.<operazione>` del catalogo, presenti solo mentre il connettore è autorizzato. Valgono gli stessi controlli degli strumenti statici (policy, SSRF, conferma delle scritture esterne, audit). Il catalogo iniziale (decisione 81) è definito dal proprietario: Gmail, Google Calendar, Outlook, GitHub, Todoist e Slack (ADR-0047).

Le competenze (ADR-0048) sono procedure del proprietario: istruzioni, parole di attivazione e strumenti suggeriti. Quando una richiesta le attiva, al massimo due, entrano nel prompt dell'agente e i loro strumenti vengono offerti per primi. Non concedono permessi: ogni strumento passa comunque da policy, conferma e audit. **Sicurezza difensiva** privilegia verifiche locali in sola lettura, evidenze, gravità motivata e mitigazioni proporzionate; non esegue exploit o scansioni su sistemi terzi. **Sviluppo e revisione del codice** prescrive lettura dei file pertinenti, riproduzione del difetto, patch circoscritta e resoconto delle verifiche effettivamente eseguite. Il briefing può riportare email o calendario solo se i relativi strumenti sono disponibili e restituiscono dati; in caso contrario dichiara la fonte non disponibile.

La sezione Connettori mostra lo schema dei parametri di ogni operazione. Il modulo genera un comando `connettore <id> <operazione> {...}` trattato in modo deterministico anche con Ollama attivo. Il Core valida lo schema e decide se procedere o chiedere conferma. Per le richieste in linguaggio naturale, i nomi di connettori e operazioni citati hanno precedenza nel budget di delega. Ollama può proporre una chiamata soltanto per le funzioni incluse nel proprio prompt.

Le proposte di auto-evoluzione passano attraverso modifiche versionate, test isolati e i controlli di rilascio. La configurazione di sicurezza resta sotto controllo del proprietario.

## Missioni e iniziative

Una **missione** (ADR-0096) è un obiettivo che JARVIS persegue da solo: lo scheduler la avvia, un agente isolato usa solo strumenti senza conferma e a rischio non alto (lettura, note e promemoria) e ogni esecuzione termina con un rapporto e una notifica. Con una chiave cloud il modello decide i passi; senza, valgono i comandi locali. Ambito, policy, ripetizioni e audit sono quelli di ogni strumento, con identità `agent:mission`; un'azione che richiederebbe conferma viene descritta nel rapporto. Le **iniziative proattive** (ADR-0097) sono regole locali e deterministiche, con ore di silenzio: briefing quotidiano e settimanale della direzione (ADR-0142), spazio disco, conferme dimenticate, automazioni che falliscono, cose da fare lasciate indietro. Il briefing non interroga i connettori.

## Stato del prototipo v0.1

Il catalogo iniziale è in `contracts/catalog/tools`, con un descrittore `jarvis.tool/1` per ogni strumento:

| Strumento | Rischio | Conferma del proprietario |
|---|---|---|
| `time.now`, `system.info`, `fs.list`, `fs.read`, `memory.list` | basso | no |
| `memory.remember` | medio | no |
| `fs.write`, `memory.forget` | alto | sì |

| `web.search`, `web.fetch` | medio, destinazione esterna | no (policy su domini e indirizzi) |
| `workflow.list` | basso | no |
| `workflow.run` | medio | no (esegue passi già approvati) |
| `workflow.create`, `workflow.update`, `workflow.delete` | alto | sì |

Ciclo di un'operazione: proposta dell'agente → validazione degli argomenti → decisione di policy → eventuale conferma → esecuzione nel Core → evento di audit → risposta composta dall'agente con il risultato. Un timeout o un crash del worker termina il task come `failed`, con un errore v1 a effetto `unknown`.

## Automazioni (Fase 3, RF-07)

Le automazioni (`jarvis.workflow/1`) eseguono uno o più passi del catalogo:
- a intervalli (minimo 1 minuto);
- ogni giorno a un'ora locale;
- una volta sola.

Garanzie:
- **approvazione:** il proprietario approva i passi esatti alla creazione, e l'approvazione è legata al loro digest;
- **policy:** a ogni esecuzione la policy vigente viene rivalutata. Un passo negato non viene eseguito; con `on_failure: stop` i passi successivi vengono saltati, con `continue` l'esito è `partial`;
- **evidenze:** ogni esecuzione (`jarvis.workflow-run/1`) registra trigger (`schedule`, `catch_up`, `manual`), istanti, decisione ed esito di ogni passo. Produce anche un task e gli eventi di audit;
- **riavvio:** le automazioni sopravvivono al riavvio; dopo uno spegnimento si recupera al più un'esecuzione persa.

Dettagli in ADR-0005.

**Modifica (ADR-0008):** nome, frequenza, passi e comportamento in caso di errore si modificano dalla chat («modifica automazione <id> ogni 2 ore», «modifica automazione <id>: info sistema», «rinomina automazione <id> in …») o dall'interfaccia. Ogni modifica crea una nuova versione con una nuova approvazione; le versioni precedenti e il legame fra esecuzione e versione restano consultabili.

**n8n locale (ADR-0053, ADR-0090):** n8n offre solo routine predefinite di promemoria e verifica dell'audit, fino a 20, pianificate ogni 1–59 minuti. I workflow sono generati dal Core con nodi fissi; non vengono importati workflow arbitrari né eseguiti codice o credenziali utente. Ogni esecuzione riuscita produce una notifica locale. n8n è disabilitato finché il proprietario non lo abilita; il Core applica la policy e mantiene il proprio scheduler per le automazioni `jarvis.workflow/1`.

Il servizio usa l'immagine versionata `docker.n8n.io/n8nio/n8n:2.48.7`, l'utente non root `1000:1000`, filesystem root in sola lettura, cache e `/tmp` temporanei, rete Docker disabilitata, capability rimosse, `no-new-privileges`, task runner disabilitati, limiti di PID/memoria/CPU e log locali ruotati. Il solo volume persistente contiene i dati n8n; il container amministrativo condivide il volume solo mentre il servizio è fermo. Le chiamate Docker sono fissate al daemon locale. Lo stato delle routine corrotto o non leggibile viene segnalato, non interpretato come archivio vuoto.

**Sandbox Python Docker:** `code.run_python` richiede conferma e limita il sorgente a 64 KiB, l'output a 8 KiB per stream e l'esecuzione a 12 secondi. Il container effimero usa l'immagine locale `python:3.13-alpine`, il daemon Docker locale, rete disabilitata, root filesystem in sola lettura, utente non root, nessun mount della workspace, capacità rimosse e limiti di processo, memoria e CPU. Non è un ambiente di produzione né una garanzia contro tutte le vulnerabilità del kernel Docker.


**Più strumenti per richiesta (ADR-0037).** L'orchestratore esegue fino a 4 strumenti in sequenza per richiesta; ognuno passa da policy, audit ed eventuale conferma, e una proposta ripetuta chiude il ciclo. Con OAuth un 401 provoca un rinnovo immediato e una sola nuova chiamata.


**Processi, terminale e specializzazioni (ADR-0038, ADR-0088).** `process.list` elenca i processi senza conferma; `shell.run` esegue un comando dopo la conferma del proprietario, dalla workspace come directory di lavoro, con limite di 60 secondi, output troncato e negazione di comandi distruttivi o di elevazione noti (`JARVIS_SHELL=off` lo disattiva). Non è una sandbox: il comando mantiene i permessi dell'utente. Il Core affida le richieste di un solo ambito a uno specialista (`research`, `system`, `automation`, `memory`, `coding`) che riceve solo gli strumenti di quell'ambito.


**Agenti temporanei (ADR-0040).** `agent.delegate` non amplia i permessi: il figlio riceve solo strumenti senza conferma, passa da ambito, policy e audit, e ciò che richiederebbe una conferma resta all'agente principale.


**Navigazione con il browser (ADR-0043).** `web.browse` apre una pagina con il browser del sistema (JavaScript incluso) e segue fino a 3 collegamenti; `web.form` compila e invia un modulo solo dopo la conferma del proprietario. Ogni richiesta di rete del browser passa dalla policy Web; il profilo è temporaneo.


**Strumenti quotidiani (ADR-0046, ADR-0084).** Calcolo, conversioni di unità, date, note, cose da fare con promemoria, ricerca e gestione dei file nella workspace (spostare ed eliminare con conferma), dischi, meteo da Open-Meteo, correzione di un ricordo. `todo.review` legge le attività scadute e quelle in scadenza entro 24 ore (orizzonte configurabile fino a 7 giorni) e propone i prossimi passi. È incluso nel briefing e nella competenza Organizzazione; non modifica le attività né invia comunicazioni.

## Terminale sandbox (ADR-0105)

`sandbox.run`, `sandbox.status` e `sandbox.reset` danno a JARVIS un terminale isolato: container Docker senza rete, radice in sola lettura, una cartella di sessione persistente in `<cartella dati>/sandbox/<sessione>`. Immagine consigliata: `infra/sandbox/build.sh` (Python, Node, compilatori, git, jq, ripgrep). Comandi senza conferma perché isolati; interruttore del proprietario `PATCH /api/sandbox` o `JARVIS_SANDBOX=off`. Esempi: «esegui nella sandbox: python3 -V», «stato sandbox», «azzera la sandbox».

**File (ADR-0106).** `sandbox.import` copia un file della workspace nella sessione; `sandbox.export` riporta un file nella workspace e chiede conferma. Esempi: «importa nella sandbox: dati.csv», «esporta dalla sandbox: out.txt in risultati/out.txt». **Avviso (ADR-0107):** JARVIS segnala una sessione oltre 200 MiB.

**Docker (ADR-0113, ADR-0114).** `sandbox.status` dice se Docker manca, è spento o non ha permessi, e come rimediare. «Prepara la sandbox» costruisce l'immagine dall'app (con conferma, serve la rete una volta sola).

**Competenze (ADR-0117, ADR-0118).** Strumenti `security.cvss`, `security.hash`, `it.subnet`, `biz.vat`, `biz.loan`; specialista `business`; competenze «Analisi e procedure aziendali», «Operazioni IT e runbook», «Risposta agli incidenti di sicurezza» e «Conformità e protezione dei dati». **Permessi (ADR-0116):** pagina «Permessi» dell'app desktop e `GET/PATCH /api/permissions`.

**Direzione aziendale (ADR-0132).** Strumenti di calcolo `biz.margin`, `biz.breakeven`, `biz.npv`, `biz.runway`, `biz.unit_economics`, `biz.priorities`, `biz.cagr` (oltre a `biz.vat` e `biz.loan`), offerti anche alla chat cloud; specialista «Direzione aziendale (CEO/CFO)» e competenze Direzione e decisioni, Controllo finanziario, Riunioni e azioni, Vendite e offerte. Tutti i calcoli sono locali, senza rete né scritture.

**Applicazioni (ADR-0121).** «apri l'app Spotify», «porta in primo piano Chrome», «scrivi nell'app: …», «premi cmd+s», «quali finestre sono aperte»: ogni azione chiede la tua conferma e mostra cosa farà. **Chiave API (ADR-0119):** incolla la chiave come capita, JARVIS la ripulisce; «Testa chiave» verifica prima le credenziali. **Personalità (ADR-0120):** stile «come nel film» o sobrio e forma di rivolgersi, in Modelli AI.

**Schermo e voce (ADR-0122, ADR-0123).** «cosa c'è sullo schermo» (con conferma; serve Tesseract) e «puoi leggere lo schermo?». La voce è solo maschile: «Leggi le risposte ad alta voce» in Modelli AI (Luca su macOS, Cosimo su Windows) e Piper con Riccardo. Comandi vocali: «Jarvis, ricordami tra venti minuti…», «stop», «ripeti».

**Chat multiple, lavoro in background e sviluppo (ADR-0143–0146).** Ogni chat ha la propria cronologia; con «In background» una richiesta cloud prosegue nel Core anche con l'app nascosta e avvisa alla fine. I ricordi si possono fissare (sempre nel contesto, non scadono). Le competenze di sviluppo (app complete, interfaccia moderna, revisione, debug, test, API, database, rilascio, sicurezza del codice) usano gli strumenti `fs.*`, `sandbox.*` e `code.*` con le stesse policy e conferme di sempre; `GET /api/openapi.json` descrive l'API.
