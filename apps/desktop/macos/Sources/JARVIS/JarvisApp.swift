import AppKit
import SwiftUI
import MapKit
import UniformTypeIdentifiers

@MainActor final class AppLifecycle: NSObject, NSApplicationDelegate {
    let core = CoreSession()
    func applicationDidBecomeActive(_ notification: Notification) {
        Task { await core.authenticateOnLaunch() }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) {
        core.stop()
    }
}

@main struct JarvisApp: App {
    @NSApplicationDelegateAdaptor(AppLifecycle.self) private var lifecycle

    var body: some Scene {
        Window("JARVIS", id: "jarvis") {
            RootView(core: lifecycle.core)
                .frame(minWidth: 580, minHeight: 510)
                .preferredColorScheme(.dark)
        }
        .defaultSize(width: 1210, height: 790)
    }
}

/// Un luogo trovato sulla mappa.
private struct MapSpot: Identifiable {
    let id = UUID()
    let name: String
    let coordinate: CLLocationCoordinate2D
}

private enum Section: String, Identifiable {
    case dashboard = "Plancia", chat = "Chat", tasks = "Attività", confirmations = "Conferme"
    case workflows = "Automazioni", memory = "Memoria", workspace = "Workspace", connectors = "Connettori"
    case devices = "Dispositivi", skills = "Competenze", security = "Sicurezza", audit = "Audit"
    case system = "Sistema", models = "Modelli AI", updates = "Aggiornamenti", notifications = "Notifiche"
    case missions = "Missioni", profile = "Profilo", plans = "Piani", tools = "Strumenti"
    case agents = "Agenti", plugins = "Pacchetti", users = "Utenti", voice = "Voce", map = "Mappa", privacy = "Privacy"
    case research = "Ricerche", datasets = "Dataset", tree = "Yggdrasill"

    /// I nove regni (ADR-0232): ogni funzione dell'app vive in uno di essi. «Yggdrasill» è l'albero, la porta d'ingresso.
    static let navigation: [(title: String, entries: [Section])] = [
        ("Yggdrasill", [.tree]),
        ("Asgard", [.dashboard, .chat, .tasks, .confirmations]),
        ("Midgard", [.profile, .users, .workspace]),
        ("Alfheim", [.models, .agents, .datasets]),
        ("Vanaheim", [.voice, .map, .skills, .plugins]),
        ("Jotunheim", [.missions, .plans, .tools, .updates]),
        ("Svartalfheim", [.workflows, .connectors, .devices, .system]),
        ("Muspelheim", [.security, .privacy]),
        ("Helheim", [.memory, .research]),
        ("Niflheim", [.audit, .notifications])
    ]

    /// Il regno a cui appartiene la pagina.
    var realmName: String {
        Section.navigation.first { $0.entries.contains(self) }?.title ?? "Yggdrasill"
    }

    var id: Self { self }

    /// Una riga che dice a cosa serve la pagina: sta sotto il titolo e non cambia nessun comportamento.
    var subtitle: String {
        switch self {
        case .dashboard: "Stato di JARVIS, attività, cose che richiedono attenzione"
        case .chat: "Parla con JARVIS: scrivi o detta, vedi le risposte in tempo reale"
        case .tasks: "Cosa sta facendo e cosa ha fatto, con l'esito di ogni passo"
        case .confirmations: "Operazioni che aspettano il tuo permesso prima di partire"
        case .workflows: "Operazioni pianificate e automazioni, anche con n8n"
        case .memory: "Cosa JARVIS ricorda: cerca, correggi o dimentica"
        case .workspace: "Cartelle di lavoro, profili e app da aprire insieme"
        case .connectors: "Servizi collegati: posta, calendario, GitHub, server MCP"
        case .devices: "Dispositivi associati e collegamento con Tailscale"
        case .skills: "Competenze e istruzioni che JARVIS applica quando servono"
        case .security: "Postura di sicurezza, cifratura e arresto di emergenza"
        case .audit: "Registro firmato di tutto ciò che è stato deciso ed eseguito"
        case .system: "Backup, risorse del computer e installazioni"
        case .models: "Modelli AI, chiavi e provider cloud"
        case .updates: "Versione installata, controllo e installazione degli aggiornamenti"
        case .notifications: "Avvisi del Core, dei connettori e delle automazioni"
        case .missions: "Obiettivi lunghi che JARVIS porta avanti per te"
        case .profile: "Le tue preferenze e come JARVIS ti conosce"
        case .plans: "Piani a passi con dipendenze, prove a secco e conferme"
        case .tools: "Strumenti disponibili, rischio e regole del proprietario"
        case .agents: "Specializzazioni, pause e limiti di lavoro"
        case .plugins: "Pacchetti che aggiungono strumenti e competenze"
        case .users: "Account, ruoli e inviti"
        case .voice: "Riconoscimento e sintesi vocale installati in locale"
        case .map: "Luoghi e percorsi su OpenStreetMap"
        case .privacy: "Cosa viene conservato, cosa può uscire e chi ha il permesso"
        case .research: "Ricerche salvate: risultati, fonti e domande aperte"
        case .datasets: "Esempi con cui JARVIS ti capisce meglio: misura, migliora, carica in privato"
        case .tree: "Il cuore di JARVIS: i nove regni, lo stato e i comandi"
        }
    }

    var icon: String {
        switch self {
        case .dashboard: "square.grid.2x2"
        case .chat: "bubble.left.and.text.bubble.right"
        case .tasks: "checklist"
        case .confirmations: "checkmark.shield"
        case .workflows: "bolt.horizontal"
        case .memory: "brain.head.profile"
        case .workspace: "folder"
        case .connectors: "link"
        case .devices: "laptopcomputer.and.iphone"
        case .skills: "wand.and.stars"
        case .security: "lock.shield.fill"
        case .audit: "list.bullet.rectangle"
        case .system: "gauge.with.dots.needle.50percent"
        case .models: "sparkles"
        case .updates: "arrow.down.circle"
        case .notifications: "bell"
        case .missions: "scope"
        case .profile: "person.crop.circle"
        case .plans: "point.3.connected.trianglepath.dotted"
        case .tools: "wrench.and.screwdriver"
        case .agents: "person.3"
        case .plugins: "shippingbox"
        case .users: "person.2"
        case .voice: "waveform.circle"
        case .map: "map"
        case .privacy: "hand.raised.square"
        case .research: "magnifyingglass.circle"
        case .datasets: "tablecells"
        case .tree: "leaf.fill"
        }
    }
    var endpoint: String {
        switch self {
        case .dashboard, .tasks: "/api/tasks?limit=20"
        case .chat: "/api/conversation?limit=100"
        case .confirmations: "/api/confirmations"
        case .workflows: "/api/workflows"
        case .memory: "/api/memory"
        case .workspace: "/api/workspace"
        case .connectors: "/api/connectors"
        case .devices: "/api/devices"
        case .skills: "/api/skills"
        case .security: "/api/security"
        case .audit: "/api/audit?limit=75"
        case .system: "/api/metrics"
        case .models: "/api/cloud"
        case .updates: "/api/updates"
        case .notifications: "/api/notifications"
        case .missions: "/api/missions"
        case .profile: "/api/profile"
        case .plans: "/api/plans"
        case .tools: "/api/tools/registry"
        case .agents: "/api/agents/supervisor"
        case .plugins: "/api/plugins"
        case .users: "/api/users"
        case .voice: "/api/setup"
        case .map: "/api/health"
        case .privacy: "/api/privacy/overview"
        case .research: "/api/research"
        case .datasets: "/api/datasets"
        case .tree: "/api/health"
        }
    }
}

private let base = Color(red: 0.025, green: 0.047, blue: 0.083)
private let panel = Color(red: 0.052, green: 0.095, blue: 0.15)
private let iris = Color(red: 0.64, green: 0.68, blue: 1)

@MainActor private struct RootView: View {
    @ObservedObject var core: CoreSession
    @State private var remoteOrigin = ""
    @State private var connectionProbe: (origin: String, detail: String, verified: Bool)?
    @State private var checkingHTTPS = false
    @State private var allowDeviceConfirmations = false
    @State private var invite: [String: Any]?
    @State private var publicHost = ""
    /// Profilo del proprietario (ADR-0133): bozza modificabile dei campi.
    @State private var profileDraft: [String: String] = [:]
    @State private var planGoal = ""
    @State private var agentLimits: [String: String] = [:]
    /// Incrementato a ogni operazione riuscita: l'avatar risponde con un'onda verde.
    @State private var celebrateTick = 0
    @State private var pluginJSON = ""
    @State private var inviteCode: String?
    @State private var inviteExpires = ""
    @State private var deleteUserID: String?
    @State private var profileShare = true
    @State private var changingInvite = false

    @State private var section: Section = .tree
    /// Regno aperto sull'albero e larghezza disponibile per scegliere il layout della pagina d'ingresso.
    @State private var realmSelection: String?
    @State private var applyingUpdate = false
    @State private var updateLog = ""
    @State private var homeWidth: CGFloat = 1200
    @State private var n8n: [String: Any] = [:]
    @State private var n8nPullConfirm = false
    @State private var n8nName = ""
    @State private var n8nMinutes = 15
    @State private var n8nKind = "reminder"
    @State private var paletteOpen = false
    @State private var paletteQuery = ""
    @State private var paletteResults: [[String: Any]] = []
    @State private var paletteTask: Task<Void, Never>?
    @Namespace private var navigationNamespace
    @State private var hoveredEntry: Section?
    /// Larghezza della barra laterale, scelta dal proprietario e conservata fra un avvio e l'altro.
    @AppStorage("sidebarWidth") private var sidebarWidth = 256.0
    @AppStorage("sidebarCollapsed") private var sidebarCollapsed = false
    /// Regni aperti nella barra laterale (quello della pagina attiva si apre da solo).
    @State private var expandedRealms: Set<String> = []
    @State private var sidebarDragStart: Double?
    /// Chat recenti mostrate nella barra laterale e finestra di modifica rapida del profilo.
    @State private var sidebarChats: [[String: Any]] = []
    @State private var profilePopover = false
    @State private var memoryStats: [String: Any] = [:]
    @State private var speakReplies = false
    @State private var systemStats: [String: Any] = [:]
    @State private var missionsPreview: [Record] = []
    /// Dati e controlli delle schede della Plancia (ADR-0181).
    @StateObject private var dash = DashboardModel()
    @StateObject private var media = NowPlaying()
    @State private var portrait: NSImage? = UserPortrait.load()
    @State private var commandText = ""
    @State private var avatarSpeaking = false
    /// Ascolto «Hey Jarvis» (ADR-0134): microfono analizzato solo per la parola di attivazione.
    @StateObject private var wake = WakeListener()
    @State private var wakeListening = true
    /// Testo della risposta che sta arrivando in streaming (ADR-0131).
    @State private var streamingReply = ""
    @State private var voiceHint = ""
    @State private var payload: [String: Any] = [:]
    @State private var message = ""
    @State private var memoryQuery = ""
    @State private var testingCloudKey = false
    @State private var savingCloudSettings = false
    @State private var confirmKeyRemoval = false
    /// Turni recenti inviati al provider scelto (ADR-0130): attivo di default, il proprietario può spegnerlo qui.
    @AppStorage("jarvis.shareContext") private var useCloudContext = true
    @AppStorage("jarvis.chatBackground") private var chatInBackground = false
    @State private var cloudModel = ""
    @State private var provider = "nvidia_nim"
    @State private var apiKey = ""
    @State private var pending: Record?
    @State private var memoryEdit: Record?
    @State private var mapQuery = ""
    @State private var mapFrom = ""
    @State private var mapTo = ""
    @State private var mapMode = "auto"
    @State private var mapSpots: [MapSpot] = []
    @State private var mapPath: [CLLocationCoordinate2D] = []
    @State private var mapSummary = ""
    @State private var mapSteps: [String] = []
    @State private var mapLink: URL?
    /// Rapporto Markdown della ricerca salvata aperta nella sezione «Ricerche» (ADR-0206).
    @State private var researchText: String?
    /// Dataset (ADR-0221): esito dell'ultima misura, repository di destinazione e caricamento in attesa di conferma.
    @State private var datasetReport: String?
    @State private var hfRepo = ""
    @State private var pendingDatasetPush: String?
    @State private var mapBusy = false
    @State private var mapCamera: MapCameraPosition = .region(MKCoordinateRegion(
        center: CLLocationCoordinate2D(latitude: 42.5, longitude: 12.5),
        span: MKCoordinateSpan(latitudeDelta: 10, longitudeDelta: 10)))
    @State private var voiceStatus: [String: Any] = [:]
    @State private var voiceStt = "small"
    @State private var voiceTts = "it_IT-riccardo-x_low"
    @State private var voiceConfirm = false
    @State private var wsProfiles: [[String: Any]] = []
    @State private var wsName = ""
    @State private var wsApps = ""
    @State private var wsDirs = ""
    @State private var wsDescription = ""
    @State private var mcpServers: [[String: Any]] = []
    @State private var mcpTools: [String: [[String: Any]]] = [:]
    @State private var mcpToken = ""
    @State private var memoryDelete: Record?
    @State private var memoryCleanup: [String: Any]?
    @State private var memoryText = ""
    @State private var error: String?
    @State private var info: String?
    @State private var sending = false
    @State private var lastReply: [String: Any]?
    @State private var ratingTask: String?
    @State private var refresh = 0
    @State private var activeLoad: UUID?
    @State private var outgoingMessage: String?
    @FocusState private var composerFocused: Bool
    @State private var selectedTemplate = ""
    @State private var clientID = ""
    @State private var credential = ""
    @State private var connectorAction: Record?
    @State private var backupPassword = ""
    @State private var missionGoal = ""
    @State private var missionEvery = ""
    @State private var missionConnectors = false
    @State private var updateToken = ""
    @State private var automaticUpdates = false

    var body: some View {
        Group {
            if core.phase == .ready && !core.uiLocked {
                GeometryReader { geometry in
                    if geometry.size.width >= 940 {
                        HStack(spacing: 0) {
                            sidebar.frame(width: sidebarCollapsed ? 68 : sidebarWidth)
                            if !sidebarCollapsed { sidebarResizer }
                            content
                        }
                    } else {
                        VStack(spacing: 0) { compactNavigation; Divider(); content }
                    }
                }
                .task {
                    while !Task.isCancelled {
                        await core.refreshHealth()
                        try? await Task.sleep(for: .seconds(12))
                    }
                }
                .task { await core.watchNotifications() }
                .task(id: wakeShouldRun) {
                    if wakeShouldRun {
                        wake.onCommand = { text in handleVoiceCommand(text) }
                        wake.start(core: core)
                    } else {
                        wake.stop()
                    }
                }
                .onChange(of: avatarSpeaking || sending) { _, busy in wake.suspended = busy }
                .task {
                    if let status = try? await core.request("/api/voice/system") {
                        speakReplies = status["speak_replies"] as? Bool ?? false
                        wakeListening = status["wake_listening"] as? Bool ?? true
                    }
                }
            } else {
                locked
            }
        }
        .background(base)
        .task {
            if NSApplication.shared.isActive { await core.authenticateOnLaunch() }
        }
        .onChange(of: core.uiLocked) { _, locked in
            if locked {
                apiKey = ""
                credential = ""
                backupPassword = ""
                updateToken = ""
                confirmKeyRemoval = false
                invite = nil
                payload = [:]
                pending = nil
                memoryEdit = nil
                connectorAction = nil
            }
        }
        .onChange(of: core.liveTick) { _, _ in
            if section == .notifications || section == .dashboard || section == .tasks { refresh += 1 }
        }
        .onChange(of: core.phase) { _, phase in
            if phase != .ready {
                apiKey = ""
                confirmKeyRemoval = false
                invite = nil
                connectionProbe = nil
            }
        }
        .sheet(item: $pending) { item in confirmationSheet(item) }
        .sheet(item: $memoryEdit) { item in
            VStack(alignment: .leading, spacing: 16) {
                Text("Modifica ricordo").font(.title2.bold())
                TextEditor(text: $memoryText).frame(height: 140)
                HStack {
                    Button("Annulla") { memoryEdit = nil }
                    Spacer()
                    Button("Salva") {
                        mutate("/api/memory/\(urlSegment(item.id))", method: "PATCH", body: ["text": memoryText])
                        memoryEdit = nil
                    }.buttonStyle(.borderedProminent)
                    .disabled(memoryText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || memoryText.count > 1000)
                }
            }.padding(24).frame(width: 430)
        }
        .sheet(item: $connectorAction) { item in connectorSheet(item) }
        .alert("Eliminare questo ricordo?", isPresented: Binding(get: { memoryDelete != nil }, set: { if !$0 { memoryDelete = nil } }),
               presenting: memoryDelete) { item in
            Button("Elimina", role: .destructive) {
                mutate("/api/memory/\(urlSegment(item.id))", method: "DELETE", body: [:])
                memoryDelete = nil
            }
            Button("Annulla", role: .cancel) { memoryDelete = nil }
        } message: { item in
            Text(String((item["text"] ?? "").prefix(300)) + "\n\nL'eliminazione è definitiva e viene registrata nell'audit.")
        }
        .alert("Pulire i ricordi inutili?", isPresented: Binding(get: { memoryCleanup != nil }, set: { if !$0 { memoryCleanup = nil } }),
               presenting: memoryCleanup) { found in
            if (found["found"] as? Int ?? 0) > 0 {
                Button("Elimina", role: .destructive) {
                    memoryCleanup = nil
                    mutate("/api/memory/cleanup", body: ["apply": true])
                }
            }
            Button("Annulla", role: .cancel) { memoryCleanup = nil }
        } message: { found in
            let count = found["found"] as? Int ?? 0
            let examples = (found["examples"] as? [String] ?? []).prefix(4).map { "• " + String($0.prefix(90)) }.joined(separator: "\n")
            Text(count == 0 ? "Non ci sono ricordi automatici inutili." : "Trovati \(count) episodi senza valore:\n\(examples)")
        }
        .sheet(isPresented: $paletteOpen) { commandPalette }
        .background {
            // Tavolozza dei comandi (⌘K): attiva solo con il Core pronto e l'interfaccia sbloccata.
            Button("Vai a…") { if core.phase == .ready && !core.uiLocked { paletteOpen = true } }
                .keyboardShortcut("k", modifiers: .command)
                .frame(width: 0, height: 0).opacity(0).accessibilityHidden(true)
        }
    }

    /// Voci della tavolozza: le sezioni dell'app, filtrate senza distinguere maiuscole e accenti.
    private var paletteMatches: [Section] {
        let all = Section.navigation.flatMap { $0.entries }
        let query = paletteQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return all }
        return all.filter { $0.rawValue.range(of: query, options: [.caseInsensitive, .diacriticInsensitive]) != nil }
    }

    private func previewMemoryCleanup() {
        error = nil
        Task {
            do {
                memoryCleanup = try await core.request("/api/memory/cleanup", method: "POST", body: ["apply": false], timeout: 30)
            } catch { self.error = error.localizedDescription }
        }
    }

    /// Ricerca globale con attesa di 300 ms dopo l'ultimo tasto: una sola richiesta per pausa, mai a ogni lettera.
    private func searchPalette(_ value: String) {
        paletteTask?.cancel()
        let text = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.count >= 2 else { paletteResults = []; return }
        paletteTask = Task {
            try? await Task.sleep(for: .milliseconds(300))
            guard !Task.isCancelled else { return }
            let encoded = text.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? text
            if let result = try? await core.request("/api/search?q=\(encoded)", timeout: 15) {
                paletteResults = result["items"] as? [[String: Any]] ?? []
            }
        }
    }

    private func openSearchHit(_ hit: [String: Any]) {
        let target: Section = {
            switch hit["section"] as? String {
            case "memory": return .memory
            case "chat": return .chat
            case "workflows": return .workflows
            default: return .tools
            }
        }()
        if target == .memory { memoryQuery = paletteQuery }
        paletteOpen = false
        paletteQuery = ""
        paletteResults = []
        select(target)
        if target == .memory { refresh += 1 }
    }

    private func openFromPalette(_ entry: Section) {
        paletteOpen = false
        paletteQuery = ""
        select(entry)
    }

    private var commandPalette: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Vai a…").font(.title3.bold())
            TextField("Cerca una sezione o un contenuto (ricordi, chat, automazioni, strumenti)", text: $paletteQuery)
                .textFieldStyle(.roundedBorder)
                .onSubmit { if let first = paletteMatches.first { openFromPalette(first) } }
                .onChange(of: paletteQuery) { _, value in searchPalette(value) }
            if paletteMatches.isEmpty && paletteResults.isEmpty {
                Text("Nessuna sezione né contenuto corrisponde.").foregroundStyle(.secondary).frame(height: 240)
            } else {
                List {
                    ForEach(paletteMatches) { entry in
                        Button { openFromPalette(entry) } label: {
                            Label(entry.rawValue, systemImage: entry.icon).frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .buttonStyle(.plain)
                    }
                    if !paletteResults.isEmpty {
                        Text("Nei tuoi contenuti").font(.caption).foregroundStyle(.secondary)
                        ForEach(Array(paletteResults.enumerated()), id: \.offset) { _, hit in
                            Button { openSearchHit(hit) } label: {
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(String((hit["title"] as? String ?? "").prefix(90)))
                                    Text(hit["group"] as? String ?? "").font(.caption2).foregroundStyle(.secondary)
                                }.frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .frame(height: 280)
            }
            HStack {
                Text("Invio apre la prima voce · Esc chiude").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Chiudi") { paletteOpen = false; paletteQuery = "" }
            }
        }
        .padding(20).frame(width: 440)
    }

    private var wakeShouldRun: Bool { core.phase == .ready && !core.uiLocked && wakeListening }

    private var wakeStatusText: String {
        switch wake.phase {
        case .off: return wakeListening ? "Avvio dell'ascolto…" : "Ascolto spento"
        case .denied: return "Microfono non consentito"
        case .unavailable: return "Microfono non disponibile"
        case .waiting: return wake.detail.isEmpty ? "In ascolto: «Hey Jarvis»" : "Ascolto non disponibile"
        case .capturing: return "Ti ascolto…"
        case .working: return "Elaboro il comando…"
        }
    }

    /// L'avatar vira al caldo con l'arresto di emergenza attivo o un errore visibile.
    /// Conferme in attesa: l'avatar si colora d'ambra finché non decidi.
    private var avatarAwaiting: Bool { (core.health["pending_confirmations"] as? Int ?? 0) > 0 }
    private var avatarOffline: Bool { core.phase == .failed }
    private var avatarAlert: Bool { (payload["emergency_stop"] as? Bool ?? false) || error != nil }

    private func select(_ value: Section) {
        guard section != value else { return }
        activeLoad = nil
        payload = [:]
        error = nil
        info = nil
        section = value
    }

    private var sidebar: some View {
        Group {
            if sidebarCollapsed { collapsedSidebar } else { expandedSidebar }
        }
        .animation(.snappy(duration: 0.28), value: sidebarCollapsed)
        .onAppear { expandedRealms.insert(section.realmName) }
        .onChange(of: section) { _, value in
            expandedRealms.insert(value.realmName)
            Task { await refreshSidebarChats() }
        }
        .task { await refreshSidebarChats(); await refreshSidebarProfile() }
    }

    private func refreshSidebarChats() async {
        if let result = try? await core.request("/api/conversations", timeout: 5) {
            sidebarChats = (result["items"] as? [[String: Any]]) ?? []
        }
    }

    private func refreshSidebarProfile() async {
        if let result = try? await core.request("/api/profile", timeout: 5) { applyProfile(result) }
    }

    /// Nuova conversazione vuota, come «New chat» di ChatGPT: la crea, la rende attiva e apre la chat.
    private func startNewChat() {
        changeChat("/api/conversations", body: [:])
    }

    private func openChat(_ id: String) {
        changeChat("/api/conversations/select", body: ["id": id])
    }

    private var newChatButton: some View {
        Button { startNewChat() } label: {
            HStack(spacing: 9) {
                Image(systemName: "square.and.pencil").font(.system(size: 14, weight: .semibold))
                Text("Nuova chat").font(.system(size: 14, weight: .semibold))
                Spacer(minLength: 0)
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 12).padding(.vertical, 9)
            .background(RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(LinearGradient(colors: [Color.cyan.opacity(0.42), Color.blue.opacity(0.28)],
                                     startPoint: .topLeading, endPoint: .bottomTrailing)))
            .overlay { RoundedRectangle(cornerRadius: 11, style: .continuous).strokeBorder(Color.cyan.opacity(0.55)) }
            .contentShape(Rectangle())
        }
        .buttonStyle(PressableStyle())
        .keyboardShortcut("n", modifiers: .command)
        .help("Nuova chat (⌘N)").accessibilityLabel("Nuova chat")
    }

    private var recentChats: some View {
        let recent = Array(sidebarChats.prefix(6))
        return VStack(alignment: .leading, spacing: 2) {
            if !recent.isEmpty {
                Text("RECENTI").font(.system(size: 10, weight: .bold)).foregroundStyle(.secondary)
                    .padding(.horizontal, 10).padding(.top, 4).padding(.bottom, 2)
                ForEach(recent.indices, id: \.self) { index in
                    let item = recent[index]
                    let active = item["active"] as? Bool == true && section == .chat
                    Button { openChat(item["id"] as? String ?? "") } label: {
                        HStack(spacing: 8) {
                            Image(systemName: "bubble.left").font(.system(size: 11)).frame(width: 18)
                            Text(item["title"] as? String ?? "Chat").font(.system(size: 12.5)).lineLimit(1)
                            Spacer(minLength: 0)
                        }
                        .foregroundStyle(active ? Color.white : Color.primary.opacity(0.8))
                        .padding(.horizontal, 10).padding(.vertical, 6)
                        .background(RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .fill(Color.white.opacity(active ? 0.12 : 0)))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(PressableStyle())
                }
            }
        }
    }

    /// Profilo in fondo alla barra: tocca per modificarlo senza lasciare la pagina.
    private var sidebarProfile: some View {
        let role = profileDraft["role"] ?? ""
        let company = profileDraft["company"] ?? ""
        let title = role.isEmpty ? "Il tuo profilo" : role
        let subtitle = company.isEmpty ? "Tocca per completarlo" : company
        return Button { profilePopover = true } label: {
            HStack(spacing: 10) {
                ZStack {
                    Circle().fill(LinearGradient(colors: [Color.cyan.opacity(0.55), Color.purple.opacity(0.4)],
                                                 startPoint: .topLeading, endPoint: .bottomTrailing))
                    Image(systemName: "person.fill").font(.system(size: 14, weight: .semibold)).foregroundStyle(.white)
                }
                .frame(width: 32, height: 32)
                VStack(alignment: .leading, spacing: 0) {
                    Text(title).font(.system(size: 12.5, weight: .semibold)).lineLimit(1)
                    Text(subtitle).font(.system(size: 10.5)).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer(minLength: 0)
                Image(systemName: "slider.horizontal.3").font(.system(size: 12)).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10).padding(.vertical, 8)
            .background(panel, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay { RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Color.white.opacity(0.10)) }
            .contentShape(Rectangle())
        }
        .buttonStyle(PressableStyle())
        .help("Modifica il tuo profilo").accessibilityLabel("Profilo: \(title). Modifica")
        .popover(isPresented: $profilePopover, arrowEdge: .trailing) { quickProfile }
    }

    private var quickProfile: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Il tuo profilo").font(.headline)
            profileField("Ruolo", "role", "Direttore operativo")
            profileField("Azienda", "company", "Nome dell'azienda")
            profileField("Competenze", "expertise", "Cosa conosci bene")
            Picker("Tono", selection: Binding(get: { profileDraft["tone"] ?? "auto" }, set: { profileDraft["tone"] = $0 })) {
                Text("Automatico").tag("auto"); Text("Formale").tag("formale")
                Text("Cordiale").tag("cordiale"); Text("Diretto").tag("diretto")
            }
            HStack {
                Button("Profilo completo") { profilePopover = false; select(.profile) }
                Spacer()
                Button("Salva", systemImage: "checkmark") {
                    var body: [String: Any] = profileDraft.reduce(into: [:]) { $0[$1.key] = $1.value }
                    body["share_with_cloud"] = profileShare
                    profilePopover = false
                    Task {
                        do {
                            _ = try await core.request("/api/profile", method: "PATCH", body: body)
                            info = "Profilo aggiornato."
                            await refreshSidebarProfile()
                        } catch { self.error = error.localizedDescription }
                    }
                }.buttonStyle(.borderedProminent)
            }
        }
        .padding(16).frame(width: 300)
    }

    private func realmAccent(_ title: String) -> Color {
        Realm.named(title)?.color ?? Color(red: 0.95, green: 0.82, blue: 0.45)
    }

    /// Barra compatta: solo icone; ogni regno è un menu con le sue pagine.
    private func railIcon(_ icon: String, _ color: Color, selected: Bool) -> some View {
        ZStack {
            RoundedRectangle(cornerRadius: 12, style: .continuous).fill(color.opacity(selected ? 0.30 : 0.10))
            RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(color.opacity(selected ? 0.95 : 0.35), lineWidth: selected ? 1.4 : 1)
            Image(systemName: icon).font(.system(size: 17, weight: .semibold)).foregroundStyle(color)
        }
        .frame(width: 46, height: 46)
        .shadow(color: color.opacity(selected ? 0.55 : 0), radius: 8)
    }

    private var collapsedSidebar: some View {
        VStack(spacing: 10) {
            Button { sidebarCollapsed = false } label: {
                Image(systemName: "sidebar.left").font(.system(size: 15, weight: .semibold)).frame(width: 34, height: 28)
            }
            .buttonStyle(PressableStyle()).help("Espandi la barra laterale").accessibilityLabel("Espandi la barra laterale")
            Button { startNewChat() } label: {
                railIcon("square.and.pencil", Color.cyan, selected: false)
            }
            .buttonStyle(PressableStyle()).help("Nuova chat (⌘N)").accessibilityLabel("Nuova chat")
            Button { select(.tree) } label: {
                railIcon("leaf.fill", Color(red: 0.95, green: 0.82, blue: 0.45), selected: section == .tree)
            }
            .buttonStyle(PressableStyle()).help("Yggdrasill").accessibilityLabel("Yggdrasill")
            Divider().padding(.horizontal, 6)
            ScrollView {
                VStack(spacing: 8) {
                    ForEach(1..<Section.navigation.count, id: \.self) { index in
                        let group = Section.navigation[index]
                        let accent = realmAccent(group.title)
                        Menu {
                            ForEach(group.entries) { entry in
                                Button { select(entry) } label: { Label(entry.rawValue, systemImage: entry.icon) }
                            }
                        } label: {
                            railIcon(Realm.named(group.title)?.icon ?? "circle", accent, selected: group.entries.contains(section))
                        }
                        .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
                        .help("\(group.title): \(Realm.named(group.title)?.epithet ?? "")")
                        .accessibilityLabel(group.title)
                    }
                }
                .padding(.vertical, 4)
            }
            .scrollIndicators(.hidden)
            Spacer(minLength: 0)
            PulsingDot(color: coreIsUp ? Mock.green : .red, size: 8).help(coreIsUp ? "Core operativo" : "Core non raggiungibile")
            Button { profilePopover = true } label: {
                Image(systemName: "person.crop.circle.fill").font(.system(size: 24)).foregroundStyle(Color.cyan.opacity(0.9))
            }
            .buttonStyle(PressableStyle()).help("Modifica il tuo profilo").accessibilityLabel("Profilo")
            .popover(isPresented: $profilePopover, arrowEdge: .trailing) { quickProfile }
        }
        .padding(.vertical, 12).padding(.horizontal, 8)
        .background(panel.opacity(0.56))
    }

    private func toggleRealm(_ title: String) {
        if expandedRealms.contains(title) { expandedRealms.remove(title) } else { expandedRealms.insert(title) }
    }

    private func sidebarRow(_ entry: Section, accent: Color) -> some View {
        Button { select(entry) } label: {
            HStack(spacing: 10) {
                Image(systemName: entry.icon).font(.system(size: 14)).frame(width: 22)
                Text(entry.rawValue).font(.system(size: 14, weight: section == entry ? .semibold : .regular))
                Spacer(minLength: 2)
                if let badge = sidebarBadge(entry) {
                    Text(badge > 99 ? "99+" : "\(badge)")
                        .font(.caption2.weight(.bold)).monospacedDigit()
                        .contentTransition(.numericText())
                        .animation(.snappy(duration: 0.25), value: badge)
                        .padding(.horizontal, 6).padding(.vertical, 1)
                        .background(entry == .confirmations ? Color.orange : Color.cyan, in: Capsule())
                        .foregroundStyle(.black)
                        .transition(.scale.combined(with: .opacity))
                        .accessibilityLabel("\(badge) in attesa")
                }
            }
            .foregroundStyle(section == entry ? Color.white : Color.primary.opacity(0.88))
            .padding(.horizontal, 10).padding(.vertical, 8)
            .background {
                ZStack {
                    if section == entry {
                        RoundedRectangle(cornerRadius: 10)
                            .fill(LinearGradient(colors: [accent.opacity(0.55), accent.opacity(0.25)],
                                                 startPoint: .leading, endPoint: .trailing))
                            .shadow(color: accent.opacity(0.35), radius: 8, y: 2)
                            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(accent.opacity(0.7)))
                            .matchedGeometryEffect(id: "navigation-highlight", in: navigationNamespace)
                    } else if hoveredEntry == entry {
                        RoundedRectangle(cornerRadius: 10).fill(Color.white.opacity(0.07))
                    }
                }
            }
            .contentShape(Rectangle())
            .animation(.easeOut(duration: 0.14), value: hoveredEntry)
        }
        .buttonStyle(PressableStyle())
        .onHover { inside in
            if inside { hoveredEntry = entry } else if hoveredEntry == entry { hoveredEntry = nil }
        }
        .accessibilityAddTraits(section == entry ? .isSelected : [])
        .id(entry)
    }

    private func realmGroup(_ group: (title: String, entries: [Section])) -> some View {
        let realm = Realm.named(group.title)
        let accent = realmAccent(group.title)
        let open = expandedRealms.contains(group.title) || group.entries.contains(section)
        let alerts = group.entries.compactMap { sidebarBadge($0) }.reduce(0, +)
        return VStack(alignment: .leading, spacing: 4) {
            Button { withAnimation(.snappy(duration: 0.25)) { toggleRealm(group.title) } } label: {
                HStack(spacing: 9) {
                    Image(systemName: realm?.icon ?? "leaf.fill").font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(accent).frame(width: 22)
                    VStack(alignment: .leading, spacing: 0) {
                        Text(group.title).font(.system(size: 13.5, weight: .semibold)).foregroundStyle(.white.opacity(0.95))
                        if let realm { Text(realm.epithet).font(.system(size: 9.5)).foregroundStyle(.secondary).lineLimit(1) }
                    }
                    Spacer(minLength: 2)
                    if alerts > 0, !open {
                        Text("\(alerts)").font(.caption2.weight(.bold)).monospacedDigit()
                            .padding(.horizontal, 6).padding(.vertical, 1)
                            .background(Color.orange, in: Capsule()).foregroundStyle(.black)
                    }
                    Image(systemName: "chevron.right").font(.system(size: 10, weight: .bold)).foregroundStyle(.secondary)
                        .rotationEffect(.degrees(open ? 90 : 0))
                }
                .padding(.horizontal, 10).padding(.vertical, 7)
                .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(accent.opacity(open ? 0.13 : 0.05)))
                .overlay { RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(accent.opacity(open ? 0.40 : 0.14)) }
                .contentShape(Rectangle())
            }
            .buttonStyle(PressableStyle())
            .accessibilityLabel("\(group.title), \(open ? "aperto" : "chiuso")")
            if open {
                HStack(alignment: .top, spacing: 8) {
                    RoundedRectangle(cornerRadius: 1).fill(accent.opacity(0.35)).frame(width: 2).padding(.leading, 18)
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(group.entries) { sidebarRow($0, accent: accent) }
                    }
                }
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
    }

    private var expandedSidebar: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                ZStack {
                    Circle().fill(LinearGradient(colors: [Color.cyan.opacity(0.35), Color.blue.opacity(0.2)],
                                                 startPoint: .topLeading, endPoint: .bottomTrailing))
                    Circle().strokeBorder(Color(red: 0.95, green: 0.82, blue: 0.45).opacity(0.7), lineWidth: 1.5)
                    Image(systemName: "leaf.fill").font(.system(size: 15, weight: .semibold)).foregroundStyle(Color(red: 0.95, green: 0.82, blue: 0.45))
                }
                .frame(width: 34, height: 34)
                VStack(alignment: .leading, spacing: 0) {
                    Text("JARVIS").font(.system(size: 17, weight: .bold, design: .rounded))
                    Text("Custode di Yggdrasill").font(.caption2).foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                Button { sidebarCollapsed = true } label: {
                    Image(systemName: "sidebar.left").font(.system(size: 14, weight: .semibold)).frame(width: 28, height: 28)
                }
                .buttonStyle(PressableStyle()).foregroundStyle(.secondary)
                .help("Riduci la barra laterale").accessibilityLabel("Riduci la barra laterale")
            }
            .padding(.horizontal, 6).padding(.top, 6)
            .accessibilityElement(children: .contain)
            newChatButton
            sidebarRow(.tree, accent: Color(red: 0.95, green: 0.82, blue: 0.45))
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 8) {
                        recentChats
                        ForEach(1..<Section.navigation.count, id: \.self) { realmGroup(Section.navigation[$0]) }
                    }
                    .padding(.vertical, 6)
                }
                .scrollIndicators(.hidden)
                // Sfumatura ai bordi: lascia intuire che l'elenco continua senza tagliare la prima voce a riposo.
                .mask(
                    VStack(spacing: 0) {
                        LinearGradient(colors: [.clear, .black], startPoint: .top, endPoint: .bottom).frame(height: 12)
                        Rectangle()
                        LinearGradient(colors: [.black, .clear], startPoint: .top, endPoint: .bottom).frame(height: 12)
                    }
                )
                // La pagina attiva si porta in vista quando cambia da un'altra parte dell'app.
                .onChange(of: section) { _, value in
                    withAnimation(.snappy(duration: 0.3)) { proxy.scrollTo(value, anchor: .center) }
                }
            }
            sidebarStatus
            sidebarProfile
        }
        .padding(12).background(panel.opacity(0.56))
        .animation(.snappy(duration: 0.28), value: section)
    }

    /// Maniglia fra barra laterale e contenuto: trascinandola si regola la larghezza (da 232 a 360 punti).
    private var sidebarResizer: some View {
        ZStack {
            Divider()
            Rectangle().fill(Color.clear).frame(width: 9)
                .contentShape(Rectangle())
                .onHover { inside in
                    if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
                }
                .gesture(
                    DragGesture(minimumDistance: 1)
                        .onChanged { drag in
                            let start = sidebarDragStart ?? sidebarWidth
                            sidebarDragStart = start
                            sidebarWidth = min(360, max(232, start + drag.translation.width))
                        }
                        .onEnded { _ in sidebarDragStart = nil }
                )
                .accessibilityHidden(true)
        }
        .frame(width: 9)
    }

    /// Contatore accanto alla voce: conferme in attesa e notifiche da leggere (solo se maggiori di zero).
    private func sidebarBadge(_ entry: Section) -> Int? {
        let key: String
        switch entry {
        case .confirmations: key = "pending_confirmations"
        case .notifications: key = "notifications_unread"
        default: return nil
        }
        guard let value = core.health[key] as? Int, value > 0 else { return nil }
        return value
    }

    /// Scheda di stato in fondo alla barra laterale: «JARVIS Online» con onda sonora.
    private var sidebarStatus: some View {
        let online = !core.health.isEmpty
        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                PulsingDot(color: online ? .green : .orange, size: 6)
                Text(online ? "JARVIS Online" : "Core non raggiungibile").font(.caption.weight(.semibold))
            }
            if wakeShouldRun {
                Label(wakeStatusText, systemImage: wake.phase == .capturing ? "mic.fill" : "mic")
                    .font(.caption2)
                    .foregroundStyle(wake.phase == .capturing ? Color.cyan : Color.secondary)
                    .symbolEffect(.pulse, isActive: wake.phase == .capturing)
            } else {
                Text(online ? "Modalità: Operativa" : "In attesa del Core").font(.caption2).foregroundStyle(.secondary)
            }
            WaveformView(active: sending || wake.phase == .capturing, barCount: 18).frame(maxWidth: .infinity).scaleEffect(y: 0.7)
        }
        .padding(10)
        .background(panel, in: RoundedRectangle(cornerRadius: 14))
        .overlay { RoundedRectangle(cornerRadius: 14).strokeBorder(Color.cyan.opacity(0.18)) }
        .accessibilityElement(children: .combine)
    }

    private var compactNavigation: some View {
        HStack {
            Text("JARVIS").font(.headline).foregroundStyle(.cyan)
            Spacer()
            Menu {
                ForEach(Section.navigation.indices, id: \.self) { index in
                    let group = Section.navigation[index]
                    SwiftUI.Section {
                        ForEach(group.entries) { entry in
                            Button { select(entry) } label: { Label(entry.rawValue, systemImage: entry.icon) }
                        }
                    } header: {
                        Text(group.title)
                    }
                }
            } label: { Label(section.rawValue, systemImage: section.icon) }
        }.padding(14).background(panel)
    }

    private var locked: some View {
        VStack(spacing: 18) {
            ZStack {
                Circle().fill(Color.cyan.opacity(0.12)).frame(width: 118, height: 118)
                Circle().stroke(Color.cyan.opacity(0.35), lineWidth: 1).frame(width: 118, height: 118)
                Image(systemName: core.uiLocked ? "lock.fill" : "lock.shield")
                    .font(.system(size: 50, weight: .light)).foregroundStyle(.cyan)
            }
            Text("JARVIS").font(.system(size: 36, weight: .bold, design: .rounded))
            Text(core.uiLocked ? "Interfaccia bloccata." : "Accedi con Touch ID o con la password del Mac.")
                .foregroundStyle(.secondary)
            if let notice = core.notice { Text(notice).foregroundStyle(.orange).multilineTextAlignment(.center) }
            if core.phase == .starting || core.phase == .authenticating {
                ProgressView(core.phase == .starting ? "Avvio del Core" : "Autenticazione macOS")
            } else if core.phase == .ready && core.uiLocked {
                Text("Il Core continua a lavorare in background.").font(.footnote).foregroundStyle(.secondary)
                Button("Sblocca JARVIS") { Task { await core.unlockInterface() } }
                    .buttonStyle(.borderedProminent).controlSize(.large)
            } else {
                Button("Sblocca JARVIS") { Task { await core.unlock() } }
                    .buttonStyle(.borderedProminent).controlSize(.large)
            }
        }.padding(30).frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var content: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Label(section.rawValue, systemImage: section.icon).font(.title2.weight(.semibold))
                        .symbolRenderingMode(.hierarchical)
                    Text(section == .tree ? section.subtitle : "\(section.realmName) · \(section.subtitle)")
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                .layoutPriority(1)
                Spacer(minLength: 8)
                Button {
                    speakReplies.toggle()
                    let value = speakReplies
                    Task { _ = try? await core.request("/api/voice/system", method: "PATCH", body: ["speak_replies": value]) }
                } label: {
                    Image(systemName: speakReplies ? "speaker.wave.2.fill" : "speaker.slash")
                }
                .help(speakReplies ? "JARVIS legge le risposte: tocca per disattivare" : "Leggi le risposte ad alta voce")
                .accessibilityLabel("Lettura delle risposte ad alta voce")
                Button { select(.memory) } label: { Image(systemName: "magnifyingglass") }
                    .help("Cerca nei ricordi").accessibilityLabel("Cerca nei ricordi")
                Button { select(.notifications) } label: {
                    if let unread = core.health["notifications_unread"] as? Int, unread > 0 {
                        Label("\(unread)", systemImage: "bell.badge")
                    } else {
                        Image(systemName: "bell")
                    }
                }
                .help("Notifiche").accessibilityLabel("Notifiche")
                .symbolEffect(.bounce, value: core.health["notifications_unread"] as? Int ?? 0)
                Button { refresh += 1 } label: {
                    if activeLoad != nil {
                        ProgressView().controlSize(.small).frame(width: 15, height: 15)
                    } else {
                        Image(systemName: "arrow.clockwise")
                    }
                }.disabled(activeLoad != nil).help("Aggiorna").accessibilityLabel("Aggiorna")
            }
            .buttonStyle(.bordered)
            .padding(.horizontal, 18).padding(.vertical, 13)
            .background(LinearGradient(colors: [panel, base], startPoint: .leading, endPoint: .trailing))
            Divider()
            if let error { banner(error, color: .orange) }
            if let info { banner(info, color: .cyan) }
            ScrollViewReader { scroll in
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        switch section {
                        case .tree: yggdrasilHome
                        case .dashboard: dashboard
                        case .chat: chat
                        // Sulle finestre molto larghe le schede non si allungano oltre una misura leggibile.
                        default: tools.frame(maxWidth: 1080, alignment: .leading)
                        }
                        if section == .chat { Color.clear.frame(height: 1).id("latest-message") }
                    }
                    .padding(18).frame(maxWidth: .infinity, alignment: .leading)
                    .id(section)
                    .transition(.opacity.combined(with: .offset(y: 10)))
                }
                .animation(.easeOut(duration: 0.22), value: section)
                .onChange(of: chatScrollAnchor, initial: true) { _, _ in
                    if section == .chat { scroll.scrollTo("latest-message", anchor: .bottom) }
                }
                .onChange(of: streamingReply) { _, _ in
                    if section == .chat { scroll.scrollTo("latest-message", anchor: .bottom) }
                }
            }
            if section == .chat {
                Divider()
                composer.padding(14).background(base)
            }
        }
        .task(id: "\(section.rawValue)-\(refresh)") { await load(section) }
    }

    private func banner(_ text: String, color: Color) -> some View {
        Text(text).font(.callout).foregroundStyle(color)
            .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 18).padding(.vertical, 6)
            .textSelection(.enabled)
    }

    private func card<Content: View>(_ title: String, icon: String, @ViewBuilder body: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(title, systemImage: icon).font(.headline).foregroundStyle(.cyan)
                .symbolRenderingMode(.hierarchical)
            body()
        }
        .padding(16).frame(maxWidth: .infinity, alignment: .leading)
        .background {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(LinearGradient(colors: [panel, panel.opacity(0.92)], startPoint: .top, endPoint: .bottom))
                .shadow(color: .black.opacity(0.22), radius: 7, y: 3)
        }
        .modifier(CardHoverGlow())
    }

    /// Scheda «Protezione» (ADR-0149): punteggio, cifratura a riposo e controlli da sistemare.
    @ViewBuilder private var protectionCard: some View {
        let security = payload["security"] as? [String: Any] ?? [:]
        let posture = security["posture"] as? [String: Any] ?? [:]
        if let score = posture["score"] as? Int {
            let color: Color = score >= 90 ? .green : (score >= 60 ? .orange : .red)
            let encryption = security["encryption"] as? [String: Any] ?? [:]
            let atRest = encryption["chat_at_rest"] as? Bool ?? false
            let items = posture["items"] as? [[String: Any]] ?? []
            card("Protezione", icon: "lock.shield.fill") {
                HStack(spacing: 14) {
                    Gauge(value: Double(score), in: 0...100) { EmptyView() } currentValueLabel: {
                        Text("\(score)").font(.system(.title3, design: .rounded).weight(.semibold)).monospacedDigit()
                    }
                    .gaugeStyle(.accessoryCircularCapacity).tint(color).frame(width: 64, height: 64)
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Livello \(posture["level"] as? String ?? "—")").font(.headline).foregroundStyle(color)
                        Label(atRest ? "Cifratura a riposo attiva · \(encryption["algorithm"] as? String ?? "AEAD")"
                                     : "Cifratura a riposo non attiva",
                              systemImage: atRest ? "lock.fill" : "lock.open")
                            .font(.callout).foregroundStyle(atRest ? .green : .orange)
                        Text("Messaggi, attività e notifiche sono cifrati sul disco.").font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                }
                ForEach(items.indices, id: \.self) { index in
                    let item = items[index]
                    let ok = item["ok"] as? Bool ?? false
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Image(systemName: ok ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                            .foregroundStyle(ok ? .green : .orange)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(item["label"] as? String ?? "")
                            if !ok { Text(item["advice"] as? String ?? "").font(.caption).foregroundStyle(.secondary) }
                        }
                    }
                }
            }
        }
    }

    private var dashboard: some View {
        VStack(alignment: .leading, spacing: 16) {
            dashboardHero.staggeredAppear(0)
            attentionCard.staggeredAppear(1)
            dashboardRow.staggeredAppear(2)
            protectionCard.staggeredAppear(3)
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 18) {
                    quickCategories
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 170), spacing: 12, alignment: .top)], spacing: 12) {
                        dashboardTile("Core", value: coreStatus, icon: "cpu", color: coreIsUp ? .green : .orange,
                                      destination: .system, order: 0)
                        dashboardTile("Intelligenza artificiale", value: aiStatus.0, icon: "sparkles", color: aiStatus.1,
                                      destination: .models, order: 1)
                        dashboardTile("Conferme in attesa", value: count("pending_confirmations"), icon: "checkmark.shield",
                                      color: (core.health["pending_confirmations"] as? Int ?? 0) > 0 ? .orange : .green,
                                      destination: .confirmations, order: 2)
                        dashboardTile("Notifiche da leggere", value: count("notifications_unread"), icon: "bell",
                                      color: (core.health["notifications_unread"] as? Int ?? 0) > 0 ? .cyan : .green,
                                      destination: .notifications, order: 3)
                        dashboardTile("Automazioni attive", value: count("automations"), icon: "bolt",
                                      color: .cyan, destination: .workflows, order: 4)
                        dashboardTile("Verifica audit", value: auditStatus.0, icon: "lock.shield",
                                      color: auditStatus.1, destination: .security, order: 5)
                    }
                    ViewThatFits(in: .horizontal) {
                        HStack(alignment: .top, spacing: 12) {
                            recentActivity.frame(minWidth: 340)
                            missionsCard.frame(minWidth: 300)
                        }
                        VStack(alignment: .leading, spacing: 12) {
                            recentActivity
                            missionsCard
                        }
                    }
                }
                .padding(.top, 10)
            } label: {
                Text("Altre sezioni e dettagli").font(.callout.weight(.medium)).foregroundStyle(.secondary)
            }
        }
        .task(id: refresh) {
            while !Task.isCancelled {
                await dash.refresh(core)
                try? await Task.sleep(for: .seconds(20))
            }
        }
        .task {
            while !Task.isCancelled {
                await dash.sampleSystem(core)
                try? await Task.sleep(for: .seconds(5))
            }
        }
    }

    /// Seconda fila della Plancia: sistema, attività, notifiche, media e monitoraggio.
    private var dashboardRow: some View {
        let notifications = dash.summary["notifications"] as? [String: Any] ?? [:]
        let unread = notifications["unread"] as? Int ?? 0
        let noteItems = notifications["items"] as? [[String: Any]] ?? []
        let cards = Group {
            SystemStatusCard(stats: dash.stats, coreUp: coreIsUp, detail: "Modello: \(activeModel)")
            TodayCard(items: dash.summary["today"] as? [[String: Any]] ?? []) { startChat("Ricordami di ") }
            NotificationsCard(unread: unread, items: Array(noteItems.prefix(3)))
            MediaCard(media: media)
            MonitoringCard(history: dash.cpuHistory, monitoring: dash.summary["monitoring"] as? [String: Any] ?? [:])
        }
        return ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: 12) { cards }.frame(minWidth: 1000)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 230), spacing: 12, alignment: .top)], spacing: 12) { cards }
        }
    }

    private var timeGreeting: String {
        switch Calendar.current.component(.hour, from: Date()) {
        case 5..<13: return "Buongiorno"
        case 13..<18: return "Buon pomeriggio"
        case 18..<23: return "Buonasera"
        default: return "Buonanotte"
        }
    }

    private var statusHeadline: String {
        let urgent = attentionItems.filter { $0.color != .cyan }.count
        switch urgent {
        case 0: return "Il tuo assistente personale è operativo."
        case 1: return "Una cosa richiede la tua attenzione."
        default: return "\(urgent) cose richiedono la tua attenzione."
        }
    }

    /// Intestazione della Plancia (ADR-0181): saluto e barra dei comandi a sinistra, avatar al centro, specialisti
    /// attivi, meteo facoltativo e progetti a destra.
    private var dashboardHero: some View {
        let rightColumn = VStack(spacing: 12) {
            ActiveAgentsCard(items: dash.summary["agents"] as? [[String: Any]] ?? [])
            WeatherCard(weather: dash.weather, configured: dash.weather["configured"] as? Bool ?? false) { place in
                Task { await dash.setWeatherPlace(core, place) }
            }
            ProjectsCard(projects: dash.summary["projects"] as? [[String: Any]] ?? [])
        }
        return ViewThatFits(in: .horizontal) {
            HStack(alignment: .center, spacing: 18) {
                VStack(alignment: .leading, spacing: 18) {
                    greetingBlock
                    WaveformView(active: sending || avatarSpeaking, barCount: 34).frame(height: 38)
                    commandBar
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                heroAvatar(size: 330).frame(width: 350)
                rightColumn.frame(width: 280)
            }
            .frame(minWidth: 980)
            VStack(alignment: .leading, spacing: 16) {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 14) {
                        greetingBlock.frame(minWidth: 200, maxWidth: .infinity, alignment: .leading)
                        heroAvatar(size: 160).frame(width: 174)
                    }
                    VStack(alignment: .leading, spacing: 10) {
                        heroAvatar(size: 124)
                        greetingBlock
                    }
                }
                commandBar
                rightColumn
            }
        }
        .padding(22)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            ZStack {
                LinearGradient(colors: [panel, base], startPoint: .topLeading, endPoint: .bottomTrailing)
                AuroraGlow()
            }
            .clipShape(RoundedRectangle(cornerRadius: 22))
        }
        .overlay { RoundedRectangle(cornerRadius: 22).strokeBorder(.cyan.opacity(0.28)) }
    }

    private var greetingBlock: some View {
        VStack(alignment: .leading, spacing: 9) {
            Label(coreStatus, systemImage: coreIsUp ? "checkmark.circle.fill" : "exclamationmark.circle")
                .font(.caption.weight(.semibold))
                .foregroundStyle(coreIsUp ? Color.green : Color.orange)
                .padding(.horizontal, 10).padding(.vertical, 5)
                .background((coreIsUp ? Color.green : Color.orange).opacity(0.10), in: Capsule())
                .accessibilityLabel("Core: \(coreStatus)")
            Text(timeGreeting).font(.title3.weight(.medium)).foregroundStyle(.secondary)
            Text("JARVIS")
                .font(.system(size: 46, weight: .bold, design: .rounded))
                .tracking(2)
                .foregroundStyle(LinearGradient(colors: [.white, .cyan],
                                                startPoint: .topLeading, endPoint: .bottomTrailing))
                .accessibilityAddTraits(.isHeader)
            if coreIsUp {
                Text(statusHeadline).font(.callout).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func heroAvatar(size: CGFloat) -> some View {
        let talking = avatarSpeaking || !streamingReply.isEmpty
        return ZStack {
            OrbitRings().frame(width: size, height: size)
            if let portrait {
                PortraitAvatar(image: portrait, speaking: talking, thinking: sending && streamingReply.isEmpty,
                               listening: wake.phase == .capturing, level: wake.level, alert: avatarAlert,
                               awaiting: avatarAwaiting, offline: avatarOffline)
                    .frame(width: size * 0.82, height: size * 0.82)
            } else {
                Avatar(thinking: sending && streamingReply.isEmpty, speaking: talking,
                       listening: wake.phase == .capturing, level: wake.level, alert: avatarAlert,
                       awaiting: avatarAwaiting, offline: avatarOffline, celebrate: celebrateTick).frame(width: size * 0.78, height: size * 0.78)
            }
        }
        .frame(width: size, height: size)
        .frame(maxWidth: .infinity)
    }

    /// Modello attivo: il primo fornitore con chiave configurata, altrimenti un invito a configurarlo.
    private var activeModel: String {
        let providers = records(payload, key: "providers")
        if let configured = providers.first(where: { $0.data["configured"] as? Bool == true }) {
            let model = configured.data["model"] as? String ?? ""
            return model.isEmpty ? providerName(configured.id) : model
        }
        return "Nessun modello configurato"
    }

    private var aiCard: some View {
        card("AI", icon: "brain") {
            Text("Modello attivo").font(.caption).foregroundStyle(.secondary)
            Button { select(.models) } label: {
                HStack(spacing: 8) {
                    PulsingDot(color: aiStatus.1, size: 5)
                    Text(activeModel).font(.callout.weight(.medium)).lineLimit(1)
                    Spacer(minLength: 4)
                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(.secondary)
                }
                .padding(.horizontal, 10).padding(.vertical, 8)
                .background(Color.white.opacity(0.05), in: RoundedRectangle(cornerRadius: 9))
                .overlay { RoundedRectangle(cornerRadius: 9).strokeBorder(Color.cyan.opacity(0.35)) }
            }
            .buttonStyle(PressableStyle())
            HStack {
                Image(systemName: "gearshape.2").foregroundStyle(.secondary)
                Text("Orchestratore").font(.callout)
                Spacer()
                Text(coreIsUp ? "Attivo" : "Non disponibile").font(.caption.weight(.semibold))
                    .padding(.horizontal, 9).padding(.vertical, 3)
                    .background((coreIsUp ? Color.green : Color.orange).opacity(0.18), in: Capsule())
                    .foregroundStyle(coreIsUp ? Color.green : Color.orange)
            }
        }
    }

    private func usageRow(_ title: String, value: Double?, color: Color) -> some View {
        HStack(spacing: 10) {
            Text(title).font(.callout).frame(width: 92, alignment: .leading)
            GeometryReader { geometry in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.white.opacity(0.08))
                    Capsule().fill(LinearGradient(colors: [color.opacity(0.75), color], startPoint: .leading, endPoint: .trailing))
                        .frame(width: geometry.size.width * CGFloat(min(max((value ?? 0) / 100, 0), 1)))
                }
            }
            .frame(height: 6)
            Text(value.map { "\(Int($0.rounded()))%" } ?? "—")
                .font(.callout.monospacedDigit()).frame(width: 42, alignment: .trailing)
        }
        .animation(.snappy, value: value)
        .accessibilityElement(children: .combine)
    }

    private var systemCard: some View {
        let cpu = systemStats["cpu_percent"] as? Double
        let memory = systemStats["memory_percent"] as? Double
        let disk = systemStats["disk_percent"] as? Double
        let busy = [cpu, memory, disk].compactMap { $0 }.contains { $0 >= 90 }
        return card("Sistema", icon: "desktopcomputer") {
            Text(busy ? "ATTENZIONE" : "OK").font(.caption2.weight(.bold))
                .padding(.horizontal, 8).padding(.vertical, 3)
                .background((busy ? Color.orange : Color.green).opacity(0.18), in: Capsule())
                .foregroundStyle(busy ? Color.orange : Color.green)
            usageRow("CPU", value: cpu, color: .blue)
            usageRow("RAM", value: memory, color: .purple)
            usageRow("Archiviazione", value: disk, color: .cyan)
        }
        .task {
            while !Task.isCancelled {
                if let stats = try? await core.request("/api/system/stats", timeout: 10) { systemStats = stats }
                try? await Task.sleep(for: .seconds(5))
            }
        }
    }

    private func startChat(_ prompt: String) {
        select(.chat)
        message = prompt
        composerFocused = true
    }

    private var agentsCard: some View {
        let agents: [(title: String, icon: String, prompt: String)] = [
            ("Coding", "chevron.left.forwardslash.chevron.right", "Aiutami con il codice: "),
            ("Sistema", "cpu", "Controlla lo stato del sistema"),
            ("Ricerca", "magnifyingglass", "Cerca sul web: "),
            ("Aziende", "briefcase", "Analisi aziendale: "),
            ("Sicurezza", "shield.lefthalf.filled", "Fai un controllo di sicurezza del sistema"),
        ]
        return card("Specialisti", icon: "person.3") {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 64), spacing: 8)], spacing: 10) {
                ForEach(agents, id: \.title) { agent in
                    Button { startChat(agent.prompt) } label: {
                        VStack(spacing: 5) {
                            Image(systemName: agent.icon).font(.system(size: 17, weight: .medium)).foregroundStyle(.cyan)
                                .frame(width: 40, height: 40)
                                .background(Color.cyan.opacity(0.10), in: Circle())
                                .overlay { Circle().strokeBorder(Color.cyan.opacity(0.35)) }
                            Text(agent.title).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    .buttonStyle(PressableStyle()).hoverLift()
                    .help("Avvia una chat con lo specialista «\(agent.title)»")
                }
            }
        }
    }

    /// Barra dei comandi: scrivi e vai direttamente in chat.
    private var commandBar: some View {
        HStack(spacing: 10) {
            Image(systemName: "text.bubble").foregroundStyle(.cyan).accessibilityHidden(true)
            TextField("Scrivi un comando…", text: $commandText)
                .textFieldStyle(.plain).font(.body).onSubmit(sendCommand)
                .accessibilityLabel("Comando per JARVIS")
            Button { sendCommand() } label: {
                Label("Invia", systemImage: "arrow.up.circle.fill")
                    .font(.callout.weight(.semibold))
                    .padding(.horizontal, 4)
            }
            .buttonStyle(.borderedProminent).tint(.cyan)
            .disabled(sending || commandText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(12)
        .background(base.opacity(0.82), in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14).strokeBorder(.cyan.opacity(0.28))
        }
        .frame(maxWidth: 760).frame(maxWidth: .infinity, alignment: .leading)
    }

    private func sendCommand() {
        let text = commandText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !sending else { return }
        commandText = ""
        select(.chat)
        message = text
        send()
    }

    private func categoryCard(_ title: String, _ subtitle: String, icon: String, color: Color, order: Int,
                              action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 7) {
                Image(systemName: icon).font(.system(size: 24, weight: .medium)).foregroundStyle(color)
                Text(title).font(.subheadline.weight(.semibold))
                Text(subtitle).font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
                    .lineLimit(2).fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, minHeight: 104).padding(.horizontal, 10).padding(.vertical, 12)
            .background(panel, in: RoundedRectangle(cornerRadius: 16))
            .overlay { RoundedRectangle(cornerRadius: 16).strokeBorder(color.opacity(0.28)) }
            .contentShape(Rectangle())
        }
        .buttonStyle(PressableStyle()).hoverLift().staggeredAppear(order)
        .accessibilityLabel("\(title). \(subtitle)")
    }

    private var quickCategories: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 12) {
            categoryCard("Sviluppo", "Codice, debug, sandbox", icon: "chevron.left.forwardslash.chevron.right",
                         color: .cyan, order: 2) { startChat("Aiutami con il codice: ") }
            categoryCard("Sistema", "Controllo e gestione", icon: "desktopcomputer", color: .blue, order: 3) { select(.system) }
            categoryCard("Automazioni", "Workflow e promemoria", icon: "bolt.fill", color: .yellow, order: 4) { select(.workflows) }
            categoryCard("Memoria", "Ricordi e relazioni", icon: "brain", color: .purple, order: 5) { select(.memory) }
            categoryCard("Missioni", "Compiti autonomi", icon: "scope", color: .green, order: 6) { select(.missions) }
            categoryCard("Altro", "Competenze e connettori", icon: "square.grid.2x2", color: .orange, order: 7) { select(.skills) }
        }
    }

    private var missionsCard: some View {
        card("Missioni in corso", icon: "scope") {
            if missionsPreview.isEmpty {
                Text("Nessuna missione attiva. Affida a JARVIS un compito ricorrente.").font(.callout).foregroundStyle(.secondary)
            }
            ForEach(missionsPreview.prefix(4)) { mission in
                let status = mission["status"] ?? "active"
                HStack(spacing: 10) {
                    Image(systemName: "scope").foregroundStyle(status == "active" ? Color.green : Color.secondary)
                    Text(mission["goal"] ?? mission.title).lineLimit(1)
                    Spacer(minLength: 6)
                    Text(missionStatusLabel(status)).font(.caption.weight(.medium))
                        .padding(.horizontal, 8).padding(.vertical, 2)
                        .background(Color.white.opacity(0.07), in: Capsule())
                }
                .padding(.vertical, 3)
                .accessibilityElement(children: .combine)
            }
            Button("Nuova missione", systemImage: "plus") { select(.missions) }
        }
        .task(id: refresh) {
            if let result = try? await core.request("/api/missions") { missionsPreview = records(result) }
        }
    }

    private func missionStatusLabel(_ status: String) -> String {
        switch status {
        case "active": return "Attiva"
        case "paused": return "In pausa"
        case "completed": return "Completata"
        case "cancelled": return "Annullata"
        default: return status
        }
    }

    /// Cose che richiedono l'attenzione del proprietario: (icona, colore, titolo, dettaglio, destinazione).
    private var attentionItems: [(icon: String, color: Color, title: String, detail: String, destination: Section)] {
        var items: [(icon: String, color: Color, title: String, detail: String, destination: Section)] = []
        let pending = core.health["pending_confirmations"] as? Int ?? 0
        if pending > 0 {
            items.append(("checkmark.shield", .orange, pending == 1 ? "Un'azione attende la tua conferma" : "\(pending) azioni attendono la tua conferma",
                          "JARVIS non procede senza il tuo consenso.", .confirmations))
        }
        if let audit = core.health["audit"] as? [String: Any], audit["chain_valid"] as? Bool == false {
            items.append(("lock.trianglebadge.exclamationmark", .red, "Il registro di audit non risulta integro",
                          "Controlla la catena e la firma del registro.", .security))
        }
        if !coreIsUp {
            items.append(("exclamationmark.triangle", .orange, "Il Core non risponde",
                          "Controlla lo stato in Sistema.", .system))
        } else if aiStatus.1 == .red {
            items.append(("sparkles", .red, "L'intelligenza artificiale non è disponibile",
                          "Verifica la chiave e il modello in Modelli AI.", .models))
        }
        let unread = core.health["notifications_unread"] as? Int ?? 0
        if unread > 0 {
            items.append(("bell.badge", .cyan, unread == 1 ? "Una notifica da leggere" : "\(unread) notifiche da leggere",
                          "Aprile per vedere cosa è successo mentre non c'eri.", .notifications))
        }
        return items
    }

    private var attentionCard: some View {
        card("Da guardare", icon: "eye") {
            let items = attentionItems
            if items.isEmpty {
                Label("Nessuna azione richiesta: servizi attivi e nessuna conferma in sospeso.",
                      systemImage: "checkmark.circle")
                    .foregroundStyle(.green)
            }
            ForEach(items.indices, id: \.self) { index in
                let item = items[index]
                Button { select(item.destination) } label: {
                    HStack(alignment: .center, spacing: 12) {
                        Image(systemName: item.icon).foregroundStyle(item.color).frame(width: 24)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(item.title).font(.body.weight(.medium))
                            Text(item.detail).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Image(systemName: "chevron.right").font(.caption).foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4).contentShape(Rectangle())
                }.buttonStyle(.plain)
                .accessibilityElement(children: .combine)
            }
        }
    }

    private var coreIsUp: Bool {
        (core.health["core"] as? [String: Any])?["status"] as? String == "up"
    }

    private var coreStatus: String {
        coreIsUp ? "Operativo" : "Non disponibile"
    }

    private var auditStatus: (String, Color) {
        let audit = core.health["audit"] as? [String: Any]
        guard let valid = audit?["chain_valid"] as? Bool else { return ("—", .secondary) }
        return valid ? ("Integro", .green) : ("Anomalia", .orange)
    }

    private func count(_ key: String) -> String {
        (core.health[key] as? Int).map(String.init) ?? "—"
    }

    private var aiStatus: (String, Color) {
        let status = (core.health["ai"] as? [String: Any])?["status"] as? String
        switch status?.lowercased() {
        case "up", "active", "ready", "online", "ok", "healthy", "configured": return ("Operativa", .green)
        case "down", "error", "failed", "offline", "unavailable", "stopped": return ("Non disponibile", .red)
        case .some: return ("In avvio", .orange)
        case .none: return ("—", .secondary)
        }
    }

    private func dashboardTile(_ title: String, value: String, icon: String, color: Color,
                               destination: Section, order: Int = 0) -> some View {
        Button { select(destination) } label: {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Image(systemName: icon).foregroundStyle(color)
                    Spacer()
                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(.secondary)
                }
                Text(value).font(.title3.weight(.semibold)).monospacedDigit()
                    .lineLimit(1).minimumScaleFactor(0.75)
                    .contentTransition(.numericText())
                    .animation(.snappy, value: value)
                Text(title).font(.caption).foregroundStyle(.secondary)
            }
            .padding(14).frame(maxWidth: .infinity, alignment: .leading)
            .background(panel, in: RoundedRectangle(cornerRadius: 12))
            .overlay { RoundedRectangle(cornerRadius: 12).strokeBorder(color.opacity(0.25)) }
            .contentShape(Rectangle())
        }
        .buttonStyle(PressableStyle())
        .hoverLift()
        .staggeredAppear(order)
        .accessibilityLabel("\(title), \(value)")
    }

    private var recentActivity: some View {
        card("Attività recenti", icon: "clock") {
            let latest = records(payload).prefix(5)
            if latest.isEmpty {
                Text(activeLoad != nil ? "Caricamento delle attività…" : "Nessuna attività recente.")
                    .foregroundStyle(.secondary)
            }
            ForEach(latest) { item in
                let state = activityState(item["state"])
                HStack(alignment: .firstTextBaseline, spacing: 10) {
                    Image(systemName: state.icon).foregroundStyle(state.color)
                    Text(item.title).lineLimit(2).frame(maxWidth: .infinity, alignment: .leading)
                    Text(state.label).font(.caption.weight(.medium)).foregroundStyle(state.color)
                        .multilineTextAlignment(.trailing)
                }
                .padding(.vertical, 5)
                .accessibilityElement(children: .combine)
            }
            Button("Apri attività", systemImage: "arrow.right") { select(.tasks) }
        }
    }

    private var providerStatus: some View {
        card("Inferenza cloud", icon: "cloud") {
            let providers = records(payload, key: "providers")
            if providers.isEmpty {
                Text("Configurazione da verificare").font(.callout).foregroundStyle(.secondary)
            }
            ForEach(providers) { provider in
                let configured = provider.data["configured"] as? Bool == true
                VStack(alignment: .leading, spacing: 4) {
                    Text(providerName(provider.id)).font(.subheadline.weight(.semibold))
                    Label(configured ? "Chiave configurata" : "Chiave mancante",
                          systemImage: configured ? "key.fill" : "key")
                        .font(.caption).foregroundStyle(configured ? iris : Color.secondary)
                }
                .accessibilityElement(children: .combine)
            }
            Button("Modelli AI", systemImage: "arrow.right") { select(.models) }
        }
    }

    private func providerName(_ id: String) -> String {
        switch id {
        case "nvidia_nim": return "NVIDIA NIM"
        default: return id
        }
    }

    private func activityState(_ value: String?) -> (label: String, icon: String, color: Color) {
        switch value {
        case "succeeded": return ("Completata", "checkmark.circle", .green)
        case "failed": return ("Fallita", "exclamationmark.circle", .orange)
        case "cancelled": return ("Annullata", "xmark.circle", .secondary)
        case "waiting_confirmation": return ("Da confermare", "checkmark.shield", .orange)
        case "running": return ("In corso", "arrow.clockwise", .cyan)
        case "planned", "pending": return ("In attesa", "clock", .secondary)
        default: return (value ?? "Stato sconosciuto", "circle", .secondary)
        }
    }

    private var chatScrollAnchor: String? {
        guard section == .chat else { return nil }
        return outgoingMessage != nil && lastReply == nil ? "pending-message" : records(payload).last?.id
    }

    /// Chat multiple e lavoro in background (ADR-0143, ADR-0144).
    @State private var chatToDelete: String?
    @State private var changingChat = false

    private var chatControls: some View {
        let chats = (payload["chats"] as? [[String: Any]]) ?? []
        let active = chats.first { $0["active"] as? Bool == true }
        return HStack(spacing: 8) {
            Menu {
                ForEach(chats.indices, id: \.self) { index in
                    let item = chats[index]
                    Button {
                        changeChat("/api/conversations/select", body: ["id": item["id"] as? String ?? ""])
                    } label: {
                        Label(item["title"] as? String ?? "Chat",
                              systemImage: item["active"] as? Bool == true ? "checkmark" : "bubble.left")
                    }
                }
            } label: {
                Label("Chat", systemImage: "clock.arrow.circlepath")
            }
            .menuStyle(.borderlessButton).fixedSize()
            Button("Nuova chat", systemImage: "square.and.pencil") {
                changeChat("/api/conversations", body: [:])
            }
            Menu {
                Button("Copia chat", systemImage: "doc.on.doc") { copyChat() }
                if let id = active?["id"] as? String {
                    Button(id == "main" ? "Svuota chat" : "Elimina chat", role: .destructive) {
                        chatToDelete = id
                    }
                }
            } label: {
                Label("Opzioni chat", systemImage: "ellipsis.circle")
            }.menuStyle(.borderlessButton)
            Toggle(isOn: $chatInBackground) {
                Label("In background", systemImage: "moon.zzz")
            }
            .toggleStyle(.button)
            .help("JARVIS lavora nel cloud anche se l'app è in secondo piano o chiusa e ti avvisa alla fine.")
        }
        .disabled(sending || changingChat)
        .confirmationDialog("Eliminare i messaggi di questa chat?", isPresented: Binding(
            get: { chatToDelete != nil }, set: { if !$0 { chatToDelete = nil } }
        ), titleVisibility: .visible, presenting: chatToDelete) { id in
            Button(id == "main" ? "Svuota chat" : "Elimina chat", role: .destructive) {
                changeChat("/api/conversations/delete", body: ["id": id])
            }
        }
    }

    /// Esporta la chat attiva in Markdown e la copia negli appunti (ADR-0150).
    private func copyChat() {
        let chats = (payload["chats"] as? [[String: Any]]) ?? []
        let active = chats.first { $0["active"] as? Bool == true }?["id"] as? String ?? "main"
        Task {
            do {
                let result = try await core.request("/api/conversations/export?id=\(urlSegment(active))")
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(result["markdown"] as? String ?? "", forType: .string)
                info = "Chat copiata negli appunti in Markdown (\(result["messages"] as? Int ?? 0) messaggi)."
                error = nil
            } catch { self.error = error.localizedDescription }
        }
    }

    private func changeChat(_ path: String, body: [String: Any]) {
        guard !sending, !changingChat else { return }
        changingChat = true
        Task {
            defer { changingChat = false }
            do {
                _ = try await core.request(path, method: "POST", body: body)
                error = nil
                if section == .chat { await load(.chat) } else { select(.chat) }
                await refreshSidebarChats()
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    private var chat: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 15) {
                Avatar(thinking: sending && streamingReply.isEmpty, speaking: avatarSpeaking || !streamingReply.isEmpty,
                       listening: wake.phase == .capturing, level: wake.level, alert: avatarAlert,
                       awaiting: avatarAwaiting, offline: avatarOffline, celebrate: celebrateTick).frame(width: 76)
                VStack(alignment: .leading, spacing: 5) {
                    Text("JARVIS").font(.title2.weight(.semibold))
                    if sending {
                        Label("Elaborazione in corso", systemImage: "waveform")
                            .foregroundStyle(.cyan)
                    } else {
                        Label(chatInBackground ? "Lavoro in background" : "Inferenza cloud",
                              systemImage: chatInBackground ? "moon.zzz" : "cloud")
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer()
                chatControls
            }
            .padding(14).frame(maxWidth: .infinity, alignment: .leading)
            .background(LinearGradient(colors: [panel, base], startPoint: .leading, endPoint: .trailing),
                        in: RoundedRectangle(cornerRadius: 16))
            .overlay { RoundedRectangle(cornerRadius: 16).strokeBorder(.cyan.opacity(0.20)) }
            let history = records(payload)
            if history.isEmpty, outgoingMessage == nil, activeLoad == nil {
                card("Inizia una conversazione", icon: "bubble.left.and.bubble.right") {
                    Text("Scrivi una richiesta o parti da un esempio.").foregroundStyle(.secondary)
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), alignment: .leading)], alignment: .leading) {
                        promptButton("Organizza una giornata", icon: "calendar",
                                     prompt: "Aiutami a organizzare la giornata. Chiedimi prima quali sono le mie priorità.")
                        promptButton("Confronta due opzioni", icon: "arrow.left.arrow.right",
                                     prompt: "Aiutami a confrontare due opzioni. Chiedimi quali sono e quali criteri contano.")
                        promptButton("Spiega un concetto", icon: "lightbulb",
                                     prompt: "Spiegami un concetto con un esempio concreto. Chiedimi quale argomento voglio approfondire.")
                    }
                }
            }
            LazyVStack(alignment: .leading, spacing: 10) {
                ForEach(history) { item in
                    chatEntry(item)
                        .transition(.asymmetric(insertion: .move(edge: .bottom).combined(with: .opacity),
                                                removal: .opacity))
                }
            }
            .animation(.spring(response: 0.4, dampingFraction: 0.85), value: history.count)
            if let confirmation = lastReply?["confirmation"] as? [String: Any] {
                Button("Esamina l’azione proposta") { pending = Record(confirmation, index: 0) }
            }
            if let outgoingMessage, lastReply == nil {
                chatBubble(outgoingMessage, user: true)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                if streamingReply.isEmpty {
                    HStack(spacing: 8) {
                        TypingIndicator()
                        Text("JARVIS sta elaborando").font(.caption).foregroundStyle(.secondary)
                    }
                    .padding(.horizontal, 14).padding(.vertical, 9)
                    .background(panel, in: Capsule())
                    .transition(.opacity.combined(with: .scale(scale: 0.9, anchor: .leading)))
                } else {
                    chatBubble(streamingReply + " ▍", user: false)
                }
            }
        }
    }

    private func chatEntry(_ item: Record) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            let text = item["content"] ?? item["text"] ?? item.title
            let user = item["role"] == "user"
            chatBubble(text, user: user)
                .contextMenu {
                    Button("Copia testo", systemImage: "doc.on.doc") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(text, forType: .string)
                    }
                    if user {
                        Button("Riprendi come bozza", systemImage: "square.and.pencil") {
                            message = text
                            composerFocused = true
                        }
                        .disabled(!message.isEmpty || sending)
                    }
                }
            if let feedback = item.data["feedback"] as? [String: Any], let id = item["task_id"] {
                replyFeedback(feedback, id: id)
            }
            if let sources = item.data["sources"] as? [[String: Any]], !sources.isEmpty {
                DisclosureGroup("Fonti della risposta") {
                    ForEach(Array(sources.enumerated()), id: \.offset) { _, source in
                        if let raw = source["url"] as? String, let url = URL(string: raw),
                           ["http", "https"].contains(url.scheme ?? "") {
                            Link(source["title"] as? String ?? raw, destination: url)
                        }
                    }
                }.padding(.horizontal, 12)
            }
        }
    }

    private func promptButton(_ title: String, icon: String, prompt: String) -> some View {
        Button {
            message = prompt
            composerFocused = true
        } label: {
            Label(title, systemImage: icon).frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.bordered)
        .disabled(sending || !message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
    }

    private func chatBubble(_ text: String, user: Bool) -> some View {
        HStack(alignment: .top, spacing: 10) {
            if user { Spacer(minLength: 0) }
            if !user {
                Image(systemName: "waveform").font(.system(size: 13, weight: .semibold)).foregroundStyle(.cyan)
                    .frame(width: 28, height: 28)
                    .background(Color.cyan.opacity(0.12), in: Circle())
                    .overlay { Circle().strokeBorder(Color.cyan.opacity(0.35)) }
                    .accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 6) {
                Text(user ? "Tu" : "JARVIS").font(.caption.weight(.semibold))
                    .foregroundStyle(user ? iris : .cyan)
                if user {
                    Text(text).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                } else {
                    MessageContent(text: text)
                }
            }
            .padding(.horizontal, 14).padding(.vertical, 11)
            .frame(maxWidth: 720, alignment: .leading)
            .background(user ? iris.opacity(0.12) : panel, in: RoundedRectangle(cornerRadius: 16))
            .overlay { RoundedRectangle(cornerRadius: 16).strokeBorder((user ? iris : .cyan).opacity(0.16)) }
            if !user { Spacer(minLength: 0) }
        }
        .padding(user ? .leading : .trailing, 32)
        .frame(maxWidth: .infinity, alignment: user ? .trailing : .leading)
        .accessibilityElement(children: .contain)
    }

    private func replyFeedback(_ feedback: [String: Any], id: String) -> some View {
        let rating = feedback["rating"] as? String
        return ViewThatFits(in: .horizontal) {
            HStack(spacing: 10) {
                replyModel(feedback)
                Spacer(minLength: 6)
                feedbackControls(id, rating: rating)
            }
            VStack(alignment: .leading, spacing: 8) {
                replyModel(feedback)
                feedbackControls(id, rating: rating)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(panel.opacity(0.72), in: RoundedRectangle(cornerRadius: 10))
    }

    private func replyModel(_ backend: [String: Any]) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(providerName(backend["provider"] as? String ?? "")).font(.subheadline.weight(.semibold))
            if let model = backend["model"] as? String {
                Text(model).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func feedbackControls(_ id: String, rating: String?) -> some View {
        HStack(spacing: 8) {
            if ratingTask == id {
                ProgressView("Salvataggio…").controlSize(.small)
            } else if let rating {
                Label(feedbackTitle(rating), systemImage: "checkmark.circle").foregroundStyle(.green)
            } else {
                Button("Utile", systemImage: "hand.thumbsup") { rateReply(id, "helpful") }
            }
            Menu(rating == nil ? "Migliora" : "Modifica valutazione", systemImage: "slider.horizontal.3") {
                Button("Utile") { rateReply(id, "helpful") }
                Button("Più breve") { rateReply(id, "concise") }
                Button("Più dettagliata") { rateReply(id, "detailed") }
                Button("Più precisa") { rateReply(id, "precise") }
            }
        }
        .font(.caption)
        .disabled(ratingTask != nil || sending)
        .help("JARVIS conserva la categoria e il riferimento alla risposta per adattare questo modello.")
    }

    private func feedbackTitle(_ rating: String?) -> String {
        switch rating {
        case "concise": "Più breve"
        case "detailed": "Più dettagliata"
        case "precise": "Più precisa"
        default: "Utile"
        }
    }

    private func rateReply(_ id: String, _ rating: String) {
        guard ratingTask == nil, !sending else { return }
        ratingTask = id
        error = nil
        Task {
            defer { ratingTask = nil }
            do {
                _ = try await core.request("/api/cloud/feedback", method: "POST",
                                           body: ["task_id": id, "rating": rating], timeout: 10)
                if section == .chat { await load(.chat) }
            } catch {
                if section == .chat { self.error = error.localizedDescription }
            }
        }
    }

    private var composer: some View {
        VStack(spacing: 8) {
            HStack {
                Label("Inferenza cloud", systemImage: "cloud")
                    .foregroundStyle(.secondary)
                Spacer()
                Toggle("Ricorda la conversazione recente", isOn: $useCloudContext)
                    .toggleStyle(.checkbox)
                    .help("Invia i turni recenti compatibili allo stesso provider per questa richiesta.")
                    .disabled(sending)
            }.font(.caption)
            HStack(spacing: 10) {
                TextField("Messaggio a JARVIS", text: $message, axis: .vertical)
                    .textFieldStyle(.plain).lineLimit(1...5)
                    .focused($composerFocused).onSubmit(send)
                Button { send() } label: {
                    Image(systemName: sending ? "ellipsis" : "arrow.up")
                        .font(.body.weight(.bold)).foregroundStyle(.white)
                        .frame(width: 30, height: 30)
                        .background(Circle().fill(sending || message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                                                  ? Color.gray.opacity(0.4) : Color.cyan))
                }
                .buttonStyle(PressableStyle())
                .disabled(sending || message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .accessibilityLabel("Invia")
            }
            .padding(.leading, 18).padding(.trailing, 8).padding(.vertical, 8)
            .background(Capsule().fill(panel))
            .overlay { Capsule().strokeBorder(Color.cyan.opacity(composerFocused ? 0.6 : 0.25)) }
            .animation(.easeOut(duration: 0.15), value: composerFocused)
        }
    }

    private func send() { sendMessage(spoken: false) }

    /// Comando pronunciato dopo «Hey Jarvis»: la risposta viene letta ad alta voce anche se la lettura è spenta.
    private func handleVoiceCommand(_ text: String) {
        guard !sending, !changingChat else { return }
        select(.chat)
        message = text
        sendMessage(spoken: true)
    }

    private func sendMessage(spoken: Bool) {
        let text = message.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !sending, !changingChat else { return }
        let includeContext = useCloudContext
        if chatInBackground {
            message = ""
            Task {
                do {
                    _ = try await core.request("/api/cloud/background", method: "POST",
                                               body: ["message": text, "provider": "auto", "use_context": includeContext])
                    info = "Lavoro avviato in background: ti avviso con una notifica quando ha finito. La risposta resterà in questa chat."
                    error = nil
                } catch {
                    self.error = error.localizedDescription
                    if message.isEmpty { message = text }
                }
            }
            return
        }
        sending = true
        error = nil
        lastReply = nil
        message = ""
        outgoingMessage = text
        Task {
            defer {
                outgoingMessage = nil
                streamingReply = ""
                sending = false
            }
            do {
                streamingReply = ""
                let result = try await core.stream("/api/cloud/chat/stream",
                                                   body: ["message": text, "provider": "auto", "use_context": includeContext]) { piece in
                    streamingReply += piece
                }
                lastReply = result
                if speakReplies || spoken, let reply = result["reply"] as? String, !reply.isEmpty {
                    // L'avatar «parla» per la durata stimata della lettura (circa 14 caratteri al secondo).
                    let duration = min(max(Double(reply.count) / 14, 1.5), 45)
                    Task {
                        avatarSpeaking = true
                        do {
                            _ = try await core.request("/api/voice/say", method: "POST", body: ["text": reply], timeout: 30)
                            try? await Task.sleep(for: .seconds(duration))
                        } catch {
                            // Prima l'errore veniva ignorato e JARVIS restava muto senza dire perché.
                            self.info = "Risposta vocale non riuscita: \(error.localizedDescription)"
                        }
                        avatarSpeaking = false
                    }
                }
                if section == .chat { await load(.chat) }
                await core.refreshHealth()
            } catch {
                self.error = error.localizedDescription
                if message.isEmpty { message = text }
                if section == .chat { composerFocused = true }
            }
        }
    }

    /// Riepilogo della memoria (ADR-0124): quanti ricordi, di che tipo e se gli indici sono in ordine.
    private var memoryOverview: some View {
        card("Stato della memoria", icon: "brain") {
            let total = memoryStats["total"] as? Int ?? 0
            let kinds = (memoryStats["by_kind"] as? [String: Any] ?? [:]).sorted { $0.key < $1.key }
            HStack(alignment: .firstTextBaseline, spacing: 18) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(total)").font(.system(size: 30, weight: .semibold, design: .rounded))
                        .monospacedDigit().foregroundStyle(.cyan)
                        .contentTransition(.numericText())
                    Text("ricordi").font(.caption).foregroundStyle(.secondary)
                }
                ForEach(kinds, id: \.key) { entry in
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(entry.value as? Int ?? 0)").font(.title3.weight(.semibold)).monospacedDigit()
                            .contentTransition(.numericText())
                        Text(memoryKindLabel(entry.key)).font(.caption).foregroundStyle(.secondary)
                    }
                }
                Spacer()
            }
            .animation(.snappy, value: total)
            if let expiring = memoryStats["expiring_within_7_days"] as? Int, expiring > 0 {
                Label("\(expiring) in scadenza entro 7 giorni", systemImage: "hourglass").font(.caption)
                    .foregroundStyle(.orange)
            }
            if memoryStats["indexes_consistent"] as? Bool == false {
                Label("Gli indici di ricerca non sono allineati: premi Reindicizza", systemImage: "exclamationmark.triangle")
                    .font(.caption).foregroundStyle(.orange)
            }
        }
        .task(id: refresh) {
            if let stats = try? await core.request("/api/memory/stats") { memoryStats = stats }
        }
    }

    private func memoryKindLabel(_ kind: String) -> String {
        switch kind {
        case "semantic": return "fatti"
        case "episodic": return "episodi"
        case "procedural": return "preferenze"
        default: return kind
        }
    }

    /// Voce maschile di sistema (ADR-0123): legge le risposte ad alta voce.
    private var voiceCard: some View {
        card("Voce di JARVIS", icon: "speaker.wave.2") {
            Toggle("Leggi le risposte ad alta voce", isOn: $speakReplies)
                .onChange(of: speakReplies) { _, value in
                    Task { _ = try? await core.request("/api/voice/system", method: "PATCH", body: ["speak_replies": value]) }
                }
            Toggle("Ascolto sempre attivo («Hey Jarvis»)", isOn: $wakeListening)
                .onChange(of: wakeListening) { _, value in
                    Task { _ = try? await core.request("/api/voice/system", method: "PATCH", body: ["wake_listening": value]) }
                }
            Text(wakeListening
                 ? "Il microfono serve solo a riconoscere «Hey Jarvis»: senza quella parola nulla viene trascritto né salvato. Dopo il comando JARVIS torna ad ascoltare soltanto la parola di attivazione."
                 : "Ascolto spento: il microfono non è in uso.")
                .font(.caption).foregroundStyle(.secondary)
            if wakeListening, wake.phase != .off { Text(wakeStatusText + (wake.detail.isEmpty ? "" : " — " + wake.detail)).font(.caption).foregroundStyle(.cyan) }
            if wakeListening, wake.engineKind == "system", wake.phase != .off {
                Text("Motore: riconoscimento vocale del Mac, sul dispositivo. Mentre ascolta, il Mac trascrive in memoria ciò che dici e JARVIS agisce solo dopo «Hey Jarvis»: nulla viene salvato né inviato. Per far ascoltare soltanto la parola di attivazione serve il motore locale del Core (voice-setup).")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Text("JARVIS parla solo con una voce maschile: Luca, le voci incluse in macOS (Eddy, Reed, Rocko) o il modello locale installato.").font(.caption).foregroundStyle(.secondary)
            if !voiceHint.isEmpty { Text(voiceHint).font(.caption).foregroundStyle(.secondary) }
            Button("Fai tacere") {
                Task { _ = try? await core.request("/api/voice/stop", method: "POST", body: [:]) }
            }
        }
        .task {
            if let status = try? await core.request("/api/voice/system") {
                speakReplies = status["speak_replies"] as? Bool ?? false
                voiceHint = status["hint"] as? String ?? ""
            }
        }
    }

    // MARK: Yggdrasill (ADR-0232)

    private func realmFunctions(_ realmID: String) -> [YggFunction] {
        guard let group = Section.navigation.first(where: { $0.title.lowercased() == realmID }) else { return [] }
        return group.entries.map {
            YggFunction(id: $0.rawValue, title: $0.rawValue, icon: $0.icon, subtitle: $0.subtitle)
        }
    }

    private func openFunction(_ id: String) {
        if let target = Section(rawValue: id) { select(target) }
    }

    private func statusRow(_ title: String, _ value: String, _ tint: Color) -> some View {
        HStack(spacing: 8) {
            Circle().fill(tint).frame(width: 6, height: 6)
            Text(title).font(.system(size: 12.5)).foregroundStyle(.white.opacity(0.92))
            Spacer(minLength: 4)
            Text(value).font(.system(size: 12, weight: .semibold).monospacedDigit()).foregroundStyle(.secondary).lineLimit(1)
        }
        .accessibilityElement(children: .combine)
    }

    private var jarvisStatusCard: some View {
        let agents = (dash.summary["agents"] as? [[String: Any]])?.count ?? 0
        let pending = core.health["pending_confirmations"] as? Int ?? 0
        let tint = coreIsUp ? Mock.green : Color.red
        return MockCard(title: "Stato di JARVIS", icon: "sparkles",
                        trailing: AnyView(HStack(spacing: 5) {
                            PulsingDot(color: tint, size: 5)
                            Text(coreIsUp ? "Operativo" : "Non raggiungibile")
                                .font(.system(size: 10.5, weight: .semibold)).foregroundStyle(tint)
                        })) {
            statusRow("Core", coreStatus, tint)
            statusRow("Intelligenza artificiale", aiStatus.0, aiStatus.1)
            statusRow("Agenti attivi", "\(agents + 1)", Mock.cyan)
            statusRow("Conferme in attesa", count("pending_confirmations"), pending > 0 ? Mock.amber : Mock.green)
        }
    }

    private var yggdrasillLeft: some View {
        VStack(spacing: 12) {
            jarvisStatusCard
            SystemStatusCard(stats: dash.stats, coreUp: coreIsUp, detail: "Modello: \(activeModel)")
            TodayCard(items: dash.summary["today"] as? [[String: Any]] ?? []) { startChat("Ricordami di ") }
        }
    }

    private var yggdrasillRight: some View {
        let quick: [(String, String, Section)] = [
            ("bubble.left.and.text.bubble.right", "Chat", .chat), ("mic", "Voce", .voice),
            ("checklist", "Attività", .tasks), ("magnifyingglass.circle", "Ricerche", .research),
            ("checkmark.shield", "Conferme", .confirmations)
        ]
        return VStack(spacing: 12) {
            VStack(spacing: 2) {
                Text("JARVIS").font(.system(size: 26, weight: .heavy, design: .rounded))
                    .foregroundStyle(LinearGradient(colors: [Mock.cyan, Color(red: 0.4, green: 0.55, blue: 1)],
                                                    startPoint: .leading, endPoint: .trailing))
                Text("Custode di Yggdrasill").font(.caption).foregroundStyle(.secondary)
            }
            heroAvatar(size: 250)
            WaveformView(active: sending || avatarSpeaking, barCount: 26).frame(height: 30)
            Text("Ciao, sono JARVIS. Yggdrasill è \(coreIsUp ? "attivo e tutti i 9 regni sono operativi" : "in attesa del Core"). \(statusHeadline) Come posso aiutarti oggi?")
                .font(.callout).foregroundStyle(.white.opacity(0.92))
                .padding(14).frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(panel.opacity(0.9)))
                .overlay { RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Mock.cyan.opacity(0.3)) }
            commandBar
            HStack(spacing: 8) {
                ForEach(Array(quick.enumerated()), id: \.offset) { _, item in
                    Button { select(item.2) } label: {
                        VStack(spacing: 5) {
                            Image(systemName: item.0).font(.system(size: 17)).foregroundStyle(Mock.cyan)
                            Text(item.1).font(.system(size: 10.5, weight: .medium)).foregroundStyle(.white.opacity(0.9))
                        }
                        .frame(maxWidth: .infinity).padding(.vertical, 10)
                        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(panel.opacity(0.85)))
                        .overlay { RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Mock.cyan.opacity(0.28)) }
                    }
                    .buttonStyle(PressableStyle()).hoverLift()
                    .accessibilityLabel(item.1)
                }
            }
        }
    }

    /// Avvisi da mostrare sul regno: conferme in attesa su Asgard, notifiche da leggere su Niflheim.
    private func realmBadge(_ realm: Realm) -> Int {
        switch realm.id {
        case "asgard": return core.health["pending_confirmations"] as? Int ?? 0
        case "niflheim": return core.health["notifications_unread"] as? Int ?? 0
        default: return 0
        }
    }

    private func realmTile(_ realm: Realm) -> some View {
        let badge = realmBadge(realm)
        let active = realmSelection == realm.id
        return Button { realmSelection = active ? nil : realm.id } label: {
            HStack(spacing: 10) {
                ZStack {
                    Circle().fill(RadialGradient(colors: [realm.color.opacity(0.55), realm.color.opacity(0.10)],
                                                 center: .center, startRadius: 2, endRadius: 28))
                    Circle().strokeBorder(realm.color.opacity(0.75), lineWidth: 1.2)
                    Image(systemName: realm.icon).font(.system(size: 17, weight: .semibold)).foregroundStyle(realm.color)
                }
                .frame(width: 44, height: 44)
                .shadow(color: realm.color.opacity(active ? 0.7 : 0.25), radius: active ? 10 : 4)
                VStack(alignment: .leading, spacing: 2) {
                    Text(realm.name).font(.system(size: 13, weight: .bold)).foregroundStyle(.white)
                    Text(realm.summary).font(.system(size: 10.5)).foregroundStyle(.secondary)
                        .lineLimit(2).multilineTextAlignment(.leading)
                    HStack(spacing: 5) {
                        Circle().fill(coreIsUp ? Mock.green : Mock.amber).frame(width: 5, height: 5)
                        Text(coreIsUp ? "Attivo" : "In attesa").font(.system(size: 10, weight: .medium))
                            .foregroundStyle(coreIsUp ? Mock.green : Mock.amber)
                        Text("· \(realmFunctions(realm.id).count) funzioni").font(.system(size: 10)).foregroundStyle(.secondary)
                        if badge > 0 {
                            Text("\(badge)").font(.system(size: 9.5, weight: .bold).monospacedDigit())
                                .padding(.horizontal, 5).padding(.vertical, 1)
                                .background(Mock.amber, in: Capsule()).foregroundStyle(.black)
                        }
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(10)
            .frame(maxWidth: .infinity, minHeight: 78, alignment: .leading)
            .background {
                ZStack {
                    RoundedRectangle(cornerRadius: 13, style: .continuous).fill(Color.white.opacity(active ? 0.10 : 0.05))
                    RealmArt(realm: realm)
                        .opacity(active ? 0.55 : 0.30)
                        .mask(LinearGradient(colors: [.clear, .black], startPoint: .leading, endPoint: .trailing))
                        .clipShape(RoundedRectangle(cornerRadius: 13, style: .continuous))
                }
            }
            .overlay { RoundedRectangle(cornerRadius: 13, style: .continuous).strokeBorder(realm.color.opacity(active ? 0.95 : 0.30)) }
            .contentShape(Rectangle())
        }
        .buttonStyle(PressableStyle())
        .accessibilityLabel("\(realm.name), \(realm.epithet). \(realm.summary)")
    }

    private var realmOverview: some View {
        MockCard(title: "I 9 Regni — panoramica rapida", icon: "circle.hexagongrid.fill") {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 215), spacing: 10, alignment: .top)], spacing: 10) {
                ForEach(Realm.all) { realmTile($0) }
            }
        }
    }

    private var yggdrasillBottomRight: some View {
        VStack(spacing: 12) {
            HStack(alignment: .top, spacing: 12) {
                ProjectsCard(projects: dash.summary["projects"] as? [[String: Any]] ?? [])
                    .frame(maxHeight: .infinity, alignment: .top)
                ActiveAgentsCard(items: dash.summary["agents"] as? [[String: Any]] ?? [])
                    .frame(maxHeight: .infinity, alignment: .top)
            }
            .fixedSize(horizontal: false, vertical: true)
            MonitoringCard(history: dash.cpuHistory, monitoring: dash.summary["monitoring"] as? [String: Any] ?? [:])
        }
    }

    /// Le stesse tre schede in una griglia, per le larghezze medie e strette.
    private var yggdrasillBottomGrid: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 250), spacing: 12, alignment: .top)], spacing: 12) {
            ProjectsCard(projects: dash.summary["projects"] as? [[String: Any]] ?? [])
            ActiveAgentsCard(items: dash.summary["agents"] as? [[String: Any]] ?? [])
            MonitoringCard(history: dash.cpuHistory, monitoring: dash.summary["monitoring"] as? [String: Any] ?? [:])
        }
    }

    /// La pagina d'ingresso: stato a sinistra, l'albero al centro, JARVIS a destra e i nove regni sotto.
    private var yggdrasilHome: some View {
        // Tre disposizioni: larga (colonne ai lati dell'albero), media (albero in alto, poi due colonne) e stretta.
        let wide = homeWidth >= 1180
        let medium = homeWidth >= 760
        let tree = YggdrasilHome(functions: realmFunctions, open: openFunction, selected: $realmSelection,
                                 minHeight: wide ? 660 : (medium ? 600 : 480))
        return VStack(spacing: 14) {
            if wide {
                HStack(alignment: .top, spacing: 14) {
                    yggdrasillLeft.frame(width: 285)
                    tree.frame(maxWidth: .infinity)
                    yggdrasillRight.frame(width: 315)
                }
                HStack(alignment: .top, spacing: 14) {
                    realmOverview.frame(maxWidth: .infinity)
                    yggdrasillBottomRight.frame(width: 560)
                }
            } else if medium {
                tree
                HStack(alignment: .top, spacing: 14) {
                    yggdrasillLeft.frame(maxWidth: .infinity)
                    yggdrasillRight.frame(maxWidth: .infinity)
                }
                realmOverview
                yggdrasillBottomGrid
            } else {
                tree
                yggdrasillRight
                yggdrasillLeft
                realmOverview
                yggdrasillBottomGrid
            }
        }
        .background {
            GeometryReader { geometry in
                Color.clear
                    .onAppear { homeWidth = geometry.size.width }
                    .onChange(of: geometry.size.width) { _, width in homeWidth = width }
            }
        }
        .task(id: refresh) {
            while !Task.isCancelled {
                await dash.refresh(core)
                try? await Task.sleep(for: .seconds(20))
            }
        }
        .task {
            while !Task.isCancelled {
                await dash.sampleSystem(core)
                try? await Task.sleep(for: .seconds(5))
            }
        }
    }

    @ViewBuilder private var tools: some View {
        switch section {
        case .memory:
            HStack {
                TextField("Cerca nei ricordi", text: $memoryQuery).onSubmit { refresh += 1 }
                Button("Cerca") { refresh += 1 }
                Button("Reindicizza") { mutate("/api/memory/reindex", body: [:]) }
                Button("Pulisci ricordi inutili", systemImage: "wand.and.sparkles") { previewMemoryCleanup() }
                    .help("Trova gli episodi automatici senza valore (ora, calcoli, saluti) e li elimina dopo la tua conferma")
                Button("Aggiungi ricordo") {
                    select(.chat)
                    message = "ricorda che "
                }
            }
            memoryOverview
        case .missions: missionTools
        case .security: emergencyCard
        case .plans:
            improvementSuggestions
            planProposal
            planTemplates
        case .agents: agentsIntro
        case .plugins: pluginInstaller
        case .users: userAdmin
        case .profile: profileEditor
        case .connectors:
            mcpCard
            connectorSetup
        case .workflows: n8nCard
        case .workspace: workspaceProfilesCard
        case .voice: voiceInstaller
        case .map: mapView
        case .privacy: privacyPanel
        case .research: researchPanel
        case .datasets: datasetsPanel
        case .tree: EmptyView()
        case .devices:
            deviceConnection
        case .models:
            voiceCard
            cloudSettings
        case .updates:
            card("Canale stabile", icon: "arrow.down.circle") {
                Toggle("Controllo automatico", isOn: $automaticUpdates)
                SecureField("Token GitHub per un repository privato", text: $updateToken)
                HStack {
                    Button("Salva impostazioni") {
                        var body: [String: Any] = ["enabled": automaticUpdates, "channel": "stable"]
                        if !updateToken.isEmpty { body["github_token"] = updateToken }
                        updateToken = ""
                        mutate("/api/updates", body: body)
                    }
                    Button("Controlla ora") { mutate("/api/updates/check", body: [:]) }
                }
            }
            updateInstaller
        case .system:
            card("Backup cifrato", icon: "lock.doc") {
                SecureField("Passphrase del backup", text: $backupPassword)
                Button("Crea backup") {
                    let passphrase = backupPassword
                    backupPassword = ""
                    mutate("/api/backup", body: ["passphrase": passphrase])
                }.disabled(backupPassword.count < 10)
            }
        case .notifications:
            Button("Segna tutte come lette") { mutate("/api/notifications/read-all", body: [:]) }
        default: EmptyView()
        }

        let items = pageRecords
        if items.isEmpty {
            if [.security, .system, .updates].contains(section), !payload.isEmpty {
                card(section.rawValue, icon: section.icon) { details(payload) }
            } else if !payload.isEmpty, section != .profile {
                Text("Nessun elemento.").foregroundStyle(.secondary)
            }
        } else {
            ForEach(items) { item in
                card(item.title, icon: section.icon) {
                    if let subtitle = item["state"] ?? item["status"] ?? item["description"] ?? item["summary"] ?? item["body"] ?? item["kind"] {
                        Text(subtitle).foregroundStyle(.secondary)
                    }
                    rowActions(item)
                    if ![.memory, .confirmations, .connectors].contains(section) {
                        DisclosureGroup("Dettagli") { details(item.data) }
                    }
                }
            }
        }
    }

    private var cloudSettings: some View {
        let entry = records(payload, key: "providers").first { $0.id == provider }
        return VStack(alignment: .leading, spacing: 14) {
            providerSettings(entry)
            adaptationProfiles(entry)
        }
    }

    private func providerSettings(_ entry: Record?) -> some View {
        card("Provider cloud", icon: "key") {
            Picker("Provider", selection: $provider) {
                Text("NVIDIA NIM").tag("nvidia_nim")
            }
            .onChange(of: provider) { _, _ in
                apiKey = ""
                cloudModel = ""
                info = nil
                error = nil
            }
            let configured = entry?.data["configured"] as? Bool == true
            let preferred = entry?.data["preferred"] as? Bool == true
            if configured {
                Label(preferred ? "Usato per la chat" : "Chiave salvata", systemImage: "key.fill")
                    .foregroundStyle(.cyan)
            }
            SecureField("Chiave API", text: $apiKey)
            if let saved = entry?["model_override"] {
                LabeledContent("Modello salvato", value: saved)
            }
            TextField("Modello personalizzato (facoltativo)", text: $cloudModel)
            if let routing = entry?.data["routing"] as? [String: String] {
                DisclosureGroup("Modelli del catalogo") {
                    VStack(alignment: .leading, spacing: 6) {
                        catalogModel("Rapido", id: routing["fast"])
                        catalogModel("Ragionamento", id: routing["reasoning"])
                        catalogModel("Codice", id: routing["coding"])
                        catalogModel("Contesto lungo", id: routing["long"])
                    }.padding(.top, 6)
                }
            }
            ViewThatFits(in: .horizontal) {
                HStack { saveCloudSettings(configured: configured); testCloudButton(configured: configured) }
                VStack(alignment: .leading) { saveCloudSettings(configured: configured); testCloudButton(configured: configured) }
            }
            if configured {
                ViewThatFits(in: .horizontal) {
                    HStack { providerActions(preferred: preferred, hasOverride: entry?["model_override"] != nil) }
                    VStack(alignment: .leading) { providerActions(preferred: preferred, hasOverride: entry?["model_override"] != nil) }
                }
            }
        }
        .disabled(savingCloudSettings || testingCloudKey)
        .confirmationDialog("Rimuovere la chiave per \(providerName(provider))?",
                            isPresented: $confirmKeyRemoval, titleVisibility: .visible) {
            Button("Rimuovi chiave", role: .destructive) { saveCloud(["remove_key": true]) }
        }
    }

    @ViewBuilder private func catalogModel(_ title: String, id: String?) -> some View {
        if let id {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text(title).font(.subheadline.weight(.medium))
                    Text(id).font(.caption.monospaced()).foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
                Spacer()
                Button("Seleziona") { cloudModel = id }
                    .disabled(cloudModel == id)
                    .accessibilityLabel("Seleziona \(title): \(id)")
            }
        }
    }

    @ViewBuilder private func providerActions(preferred: Bool, hasOverride: Bool) -> some View {
        if !preferred {
            Button("Usa per la chat") { saveCloud(["use_for_chat": true]) }
        }
        if hasOverride {
            Button("Ripristina modelli automatici") { saveCloud(["clear_model": true]) }
        }
        Button("Rimuovi chiave", role: .destructive) { confirmKeyRemoval = true }
    }

    private func adaptationProfiles(_ entry: Record?) -> some View {
        card("Adattamento delle risposte", icon: "slider.horizontal.3") {
            Text("JARVIS adatta lo stile usando fino a 12 valutazioni recenti per modello. Al provider arriva solo una breve istruzione di stile.")
                .font(.callout).foregroundStyle(.secondary)
            let profiles = (entry?.data["feedback_profiles"] as? [[String: Any]]) ?? []
            if profiles.isEmpty {
                Label("Valuta una risposta cloud dalla chat per iniziare.", systemImage: "hand.thumbsup")
                    .foregroundStyle(.secondary)
            }
            ForEach(Array(profiles.enumerated()), id: \.offset) { _, profile in
                if let model = profile["model"] as? String {
                    Divider()
                    VStack(alignment: .leading, spacing: 5) {
                        Text(model).font(.subheadline.monospaced().weight(.semibold)).textSelection(.enabled)
                        let count = profile["count"] as? Int ?? 0
                        let length = profile["length"] as? String ?? "standard"
                        let style = length == "standard" ? "Stile standard" : feedbackTitle(length)
                        let precision = profile["precision"] as? Bool == true ? " · Più precisa" : ""
                        Text("\(count) \(count == 1 ? "valutazione" : "valutazioni") · \(style)\(precision)")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    private func saveCloudSettings(configured: Bool) -> some View {
        let key = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        let model = cloudModel.trimmingCharacters(in: .whitespacesAndNewlines)
        let title = key.isEmpty ? "Salva modello" : "Salva chiave"
        return Button(savingCloudSettings ? "Salvataggio…" : title, systemImage: "checkmark") {
            var changes: [String: Any] = [:]
            if !key.isEmpty { changes["key"] = key }
            if !model.isEmpty { changes["model"] = model }
            saveCloud(changes, clearFields: true)
        }
        .buttonStyle(.borderedProminent)
        .disabled(key.isEmpty && (!configured || model.isEmpty))
    }

    private func saveCloud(_ changes: [String: Any], clearFields: Bool = false) {
        guard !savingCloudSettings, !testingCloudKey else { return }
        let selected = provider
        var body = changes
        body["provider"] = selected
        savingCloudSettings = true
        error = nil
        info = nil
        Task {
            defer { savingCloudSettings = false }
            do {
                _ = try await core.request("/api/cloud", method: "POST", body: body)
                if clearFields && provider == selected {
                    apiKey = ""
                    cloudModel = ""
                }
                if section == .models {
                    await load(.models)
                    if section == .models { info = "Impostazioni cloud salvate." }
                }
            } catch {
                if section == .models { self.error = error.localizedDescription }
            }
        }
    }

    private func testCloudButton(configured: Bool) -> some View {
        Button(testingCloudKey ? "Verifica in corso…" : "Testa chiave", systemImage: "checkmark.shield") {
            let selected = provider
            testingCloudKey = true
            error = nil
            info = nil
            Task {
                defer { testingCloudKey = false }
                do {
                    let result = try await core.request("/api/cloud/test", method: "POST",
                                                        body: ["provider": selected], timeout: 125)
                    if section == .models {
                        info = "Richiesta di prova riuscita · \(result["model"] as? String ?? providerName(selected))"
                    }
                } catch {
                    if section == .models { self.error = error.localizedDescription }
                }
            }
        }.disabled(testingCloudKey || !configured ||
                   !apiKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
                   !cloudModel.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
    }

    private var pageRecords: [Record] {
        switch section {
        case .audit: return records(payload, key: "events")
        case .models: return records(payload, key: "providers")
        case .workspace: return records(payload, key: "entries")
        default: return records(payload)
        }
    }

    @ViewBuilder private func rowActions(_ item: Record) -> some View {
        switch section {
        case .confirmations:
            Text(item["expected_effect"] ?? item["tool"] ?? "Azione da verificare")
            Button("Esamina") { pending = item }.buttonStyle(.borderedProminent)
        case .memory:
            HStack {
                let pinned = item.data["pinned"] as? Bool ?? false
                Menu {
                    Button("Modifica", systemImage: "pencil") { memoryText = item["text"] ?? ""; memoryEdit = item }
                    Button(pinned ? "Libera" : "Fissa", systemImage: pinned ? "pin.slash" : "pin") {
                        mutate("/api/memory/\(urlSegment(item.id))/pin", body: ["pinned": !pinned])
                    }
                    Divider()
                    Button("Elimina…", systemImage: "trash", role: .destructive) { memoryDelete = item }
                } label: {
                    Label("Azioni", systemImage: "ellipsis.circle")
                }
                .menuStyle(.borderlessButton).fixedSize()
                Button("Elimina", systemImage: "trash", role: .destructive) { memoryDelete = item }
                    .help("Elimina questo ricordo")
            }
        case .tasks:
            Button("Riprendi in chat") { select(.chat); message = item.title }
        case .workflows:
            HStack {
                let workflow = item.data["workflow"] as? [String: Any] ?? item.data
                let id = urlSegment(workflow["workflow_id"] as? String ?? workflow["id"] as? String ?? item.id)
                let enabled = workflow["enabled"] as? Bool ?? false
                Button(enabled ? "Sospendi" : "Attiva") {
                    mutate("/api/workflows/\(id)/enabled", body: ["enabled": !enabled])
                }
                Button("Chiedi di eseguire") { select(.chat); message = "esegui automazione \(id)" }
            }
        case .connectors:
            HStack {
                let authorized = item.data["authorized"] as? Bool == true
                Label(authorized ? "Autorizzato" : "Da autorizzare",
                      systemImage: authorized ? "checkmark.circle" : "key")
                    .foregroundStyle(authorized ? Color.green : .orange)
                Spacer()
                Button("Configura") { credential = ""; connectorAction = item }
            }
        case .devices:
            let revoked = item["revoked_at"] != nil
            Label(revoked ? "Revocato" : "Associato", systemImage: revoked ? "lock.fill" : "checkmark.circle")
                .font(.caption.weight(.medium)).foregroundStyle(revoked ? Color.orange : .green)
            if let scopes = item.data["scopes"] as? [String], !scopes.isEmpty {
                LabeledContent("Permessi", value: scopes.map { $0 == "confirm" ? "Conferme" : $0.capitalized }.joined(separator: ", "))
            }
            if !revoked {
                Button("Revoca accesso", role: .destructive) {
                    pending = Record(["id": item.id, "tool": "devices.revoke", "target": item.title,
                                      "expected_effect": "Il dispositivo perderà l’accesso a JARVIS.",
                                      "risk": "high"], index: 0)
                }
            }
        case .notifications:
            Button("Segna come letta") {
                mutate("/api/notifications/\(urlSegment(item.id))/read", body: [:])
            }
        case .tools:
            let name = item.id
            let rule = item["owner_rule"] ?? ""
            let health = (item.data["health"] as? [String: Any])?["state"] as? String ?? "unknown"
            HStack {
                Text("Rischio \(item["risk"] ?? "?") · \(item["category"] ?? "")").font(.caption).foregroundStyle(.secondary)
                Text(["healthy": "in salute", "degraded": "instabile", "failing": "in errore"][health] ?? "senza dati")
                    .font(.caption).foregroundStyle(health == "failing" ? .red : (health == "degraded" ? .orange : .green))
                Spacer()
                Button(rule == "deny" ? "Sblocca" : "Blocca") {
                    let value: Any = rule == "deny" ? NSNull() : "deny"
                    mutate("/api/permissions", method: "PATCH", body: ["rules": [name: value]])
                }
            }
        case .agents:
            let name = item.id
            let enabled = item.data["enabled"] as? Bool ?? true
            let state = item["state"] ?? "attivo"
            let used = item.data["runs_last_hour"] as? Int ?? 0
            let limit = item.data["max_runs_per_hour"] as? Int
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(state).font(.caption.weight(.medium))
                        .foregroundStyle(state == "attivo" ? Color.green : (state == "in pausa" ? .orange : .red))
                    Text("\(used) lavori nell'ultima ora" + (limit.map { " su \($0)" } ?? ""))
                        .font(.caption).foregroundStyle(.secondary)
                }
                if let performance = item.data["performance"] as? [String: Any], (performance["runs"] as? Int ?? 0) > 0 {
                    Text("\(performance["runs"] as? Int ?? 0) lavori in totale · \(performance["success_rate"] as? Double ?? 0)% di successi")
                        .font(.caption).foregroundStyle(.secondary)
                }
                HStack {
                    Button(enabled ? "Metti in pausa" : "Riattiva", systemImage: enabled ? "pause.fill" : "play.fill") {
                        mutate("/api/agents/limits", body: ["agent": name, "enabled": !enabled])
                    }
                    TextField("senza limite", text: Binding(get: { agentLimits[name] ?? "" }, set: { agentLimits[name] = $0 }))
                        .textFieldStyle(.roundedBorder).frame(width: 110)
                    Button("Imposta il limite orario") {
                        let value = (agentLimits[name] ?? "").trimmingCharacters(in: .whitespaces)
                        if value.isEmpty {
                            mutate("/api/agents/limits", body: ["agent": name, "max_runs_per_hour": NSNull()])
                        } else if let n = Int(value), (1...500).contains(n) {
                            mutate("/api/agents/limits", body: ["agent": name, "max_runs_per_hour": n])
                        } else {
                            error = "Il limite orario va da 1 a 500 (vuoto per toglierlo)"
                            return
                        }
                        agentLimits[name] = ""
                    }
                }
            }
        case .users:
            let disabled = item.data["disabled"] as? Bool ?? false
            let messages = item.data["messages"] as? Int ?? 0
            let sessions = item.data["sessions"] as? Int ?? 0
            let last = String((item["last_login_at"] ?? "mai").prefix(16))
            VStack(alignment: .leading, spacing: 6) {
                Text("@\(item["username"] ?? "") · \(messages) messaggi · \(sessions) sessioni attive · ultimo accesso \(last)")
                    .font(.caption).foregroundStyle(.secondary)
                HStack {
                    Button(disabled ? "Riattiva" : "Disattiva", systemImage: disabled ? "play.fill" : "pause.fill") {
                        mutate("/api/users/\(urlSegment(item.id))/\(disabled ? "enable" : "disable")", body: [:])
                    }
                    if deleteUserID == item.id {
                        Text("Elimina l'account e tutti i suoi dati?").font(.caption).foregroundStyle(.red)
                        Button("Sì, elimina", role: .destructive) {
                            deleteUserID = nil
                            mutate("/api/users/\(urlSegment(item.id))/delete", body: [:])
                        }
                        Button("Annulla") { deleteUserID = nil }
                    } else {
                        Button("Elimina…", role: .destructive) { deleteUserID = item.id }
                    }
                }
            }
        case .plugins:
            let skills = (item.data["skills"] as? [Any])?.count ?? 0
            let templates = (item.data["templates"] as? [Any])?.count ?? 0
            let digest = String((item["digest"] ?? "").prefix(12))
            HStack {
                Text("v\(item["version"] ?? "?") · \(skills) competenze · \(templates) procedure · impronta \(digest)")
                    .font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Rimuovi", role: .destructive) { mutate("/api/plugins/remove", body: ["name": item.id]) }
            }
        case .plans:
            let id = urlSegment(item.id)
            let status = item["status"] ?? ""
            HStack {
                if ["pending", "interrupted", "waiting"].contains(status) {
                    Button("Esegui") { mutate("/api/plans/\(id)/run", body: [:]) }.buttonStyle(.borderedProminent)
                }
                if status == "waiting" {
                    Button("Approva i passi in attesa") { mutate("/api/plans/\(id)/approve", body: [:]) }
                }
                if ["pending", "running", "waiting", "interrupted"].contains(status) {
                    Button("Annulla") { mutate("/api/plans/\(id)/cancel", body: [:]) }
                }
                Button("Replay") { mutate("/api/plans/\(id)/replay", body: [:]) }
                if ["failed", "cancelled", "interrupted"].contains(status) {
                    Button("Ripianifica", systemImage: "arrow.triangle.2.circlepath") {
                        mutate("/api/plans/\(id)/replan", body: ["create": true])
                    }
                    .help("Il modello propone un piano di recupero: resta una bozza da rivedere")
                }
            }
        case .missions:
            let id = urlSegment(item.id)
            let status = item["status"] ?? ""
            let runs = item.data["runs"] as? Int ?? 0
            let limit = item.data["max_runs"] as? Int ?? 0
            let label = ["active": "Attiva", "paused": "In pausa", "completed": "Completata",
                         "failed": "Interrotta", "cancelled": "Annullata"][status] ?? status
            VStack(alignment: .leading, spacing: 8) {
                Text("\(label) · \(runs) di \(limit) esecuzioni")
                    .font(.caption.weight(.medium)).foregroundStyle(.cyan)
                if let report = item["result"] ?? (item.data["journal"] as? [[String: Any]])?.last?["summary"] as? String {
                    Text(report).foregroundStyle(.secondary)
                }
                HStack {
                    if status == "active" {
                        Button("Esegui ora") { mutate("/api/missions/\(id)/run", body: [:]) }
                        Button("Pausa") { mutate("/api/missions/\(id)/pause", body: [:]) }
                        Button("Annulla", role: .destructive) { mutate("/api/missions/\(id)/cancel", body: [:]) }
                    } else if status == "paused" {
                        Button("Riprendi") { mutate("/api/missions/\(id)/resume", body: [:]) }
                        Button("Annulla", role: .destructive) { mutate("/api/missions/\(id)/cancel", body: [:]) }
                    } else {
                        Button("Elimina", role: .destructive) { mutate("/api/missions/\(id)/delete", body: [:]) }
                    }
                }
            }
        default: EmptyView()
        }
    }

    /// Missioni autonome e iniziative proattive (ADR-0096, ADR-0097).
    /// Procedure pronte (ADR-0159): si avviano con un gesto; quelle con parametri obbligatori si creano da API.
    @ViewBuilder private var planTemplates: some View {
        let templates = (payload["templates"] as? [[String: Any]]) ?? []
        if !templates.isEmpty {
            card("Procedure pronte", icon: "play.rectangle") {
                ForEach(templates.indices, id: \.self) { index in
                    let template = templates[index]
                    let params = (template["params"] as? [[String: Any]]) ?? []
                    let needsInput = params.contains { $0["required"] as? Bool == true }
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(template["name"] as? String ?? "").font(.headline)
                            Text(template["description"] as? String ?? "").font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Crea e avvia", systemImage: "play.fill") {
                            mutate("/api/plans/from-template", body: ["template": template["name"] as? String ?? "", "run": true])
                        }
                        .disabled(needsInput)
                        .help(needsInput ? "Richiede parametri: crealo da API con POST /api/plans/from-template" : "Avvia la procedura")
                    }
                }
            }
        }
    }

    /// Utenti (ADR-0169): chi può registrarsi, inviti e uso del modello cloud. Il testo dei messaggi non si vede.
    private var userAdmin: some View {
        let settings = payload["settings"] as? [String: Any] ?? [:]
        let registration = settings["registration"] as? String ?? "invite"
        let cloud = settings["cloud"] as? Bool ?? false
        return VStack(spacing: 14) {
            card("Utenti e accesso", icon: "person.2") {
                Text("Le persone che inviti hanno un account con la loro conversazione, i loro ricordi e la loro personalità. Non hanno strumenti e non vedono i tuoi dati; il testo dei loro messaggi non è visibile da qui. Accedono con l'app JARVIS per Windows e macOS (egui) o dal client web, non da questa console.")
                    .font(.callout).foregroundStyle(.secondary)
                Picker("Chi può registrarsi", selection: Binding(
                    get: { registration },
                    set: { mutate("/api/users/settings", body: ["registration": $0]) }
                )) {
                    Text("Nessuno").tag("closed")
                    Text("Solo su invito").tag("invite")
                    Text("Chiunque").tag("open")
                }
                .pickerStyle(.segmented)
                Toggle("Gli utenti possono usare il mio modello cloud (costo tuo; i loro messaggi vanno al fornitore)", isOn: Binding(
                    get: { cloud },
                    set: { mutate("/api/users/settings", body: ["cloud": $0]) }
                ))
            }
            card("Inviti", icon: "ticket") {
                Button("Crea un invito", systemImage: "plus") { createInvite() }
                    .buttonStyle(.borderedProminent)
                if let code = inviteCode {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Codice di invito (si vede solo ora)").font(.caption.weight(.semibold))
                        Text(code).font(.system(.body, design: .monospaced)).textSelection(.enabled)
                        Text("Vale una volta e scade: \(inviteExpires)").font(.caption).foregroundStyle(.secondary)
                        Button("Copia", systemImage: "doc.on.doc") {
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(code, forType: .string)
                        }
                    }
                }
            }
        }
    }

    private func createInvite() {
        error = nil
        info = nil
        Task {
            do {
                let result = try await core.request("/api/users/invites", method: "POST", body: [:])
                inviteCode = result["code"] as? String
                inviteExpires = result["expires_at"] as? String ?? ""
                info = result["message"] as? String ?? "Invito creato."
                celebrateTick += 1
            } catch { self.error = error.localizedDescription }
        }
    }

    /// Supervisione degli agenti (ADR-0164).
    private var agentsIntro: some View {
        card("Supervisione degli agenti", icon: "person.3") {
            Text("Ogni specializzazione lavora solo sul proprio ambito. Puoi metterla in pausa o darle un limite di lavori all'ora: quando è ferma, la richiesta passa al generalista, sempre sotto le stesse policy e conferme. La supervisione restringe e non allarga.")
                .font(.callout).foregroundStyle(.secondary)
            if let conflicts = payload["conflicts"] as? String {
                Text(conflicts).font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    /// Pacchetti di estensione (ADR-0165): competenze e procedure in JSON, senza codice.
    private var pluginInstaller: some View {
        card("Installa o aggiorna un pacchetto", icon: "shippingbox") {
            Text("Un pacchetto aggiunge competenze e procedure scritte in JSON. Non contiene codice, non usa la rete e non porta chiavi: ciò che porta resta sotto le tue regole, conferme e audit. Si installa tutto o niente.")
                .font(.callout).foregroundStyle(.secondary)
            TextEditor(text: $pluginJSON)
                .font(.system(.caption, design: .monospaced))
                .frame(minHeight: 130)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(.secondary.opacity(0.3)))
            Button("Installa", systemImage: "square.and.arrow.down") {
                guard let object = try? JSONSerialization.jsonObject(with: Data(pluginJSON.utf8)) as? [String: Any] else {
                    error = "JSON non valido: il pacchetto deve essere un oggetto"
                    return
                }
                pluginJSON = ""
                mutate("/api/plugins", body: object)
            }
            .buttonStyle(.borderedProminent)
            .disabled(pluginJSON.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
    }

    /// Miglioramenti suggeriti (ADR-0163): JARVIS propone, il proprietario decide. Nulla cambia da solo.
    @ViewBuilder private var improvementSuggestions: some View {
        let items = (payload["improvements"] as? [[String: Any]]) ?? []
        if !items.isEmpty {
            card("Miglioramenti suggeriti", icon: "lightbulb") {
                ForEach(items.indices, id: \.self) { index in
                    let item = items[index]
                    let id = item["id"] as? String ?? ""
                    VStack(alignment: .leading, spacing: 4) {
                        Text(item["title"] as? String ?? "").font(.headline)
                        Text(item["detail"] as? String ?? "").font(.caption).foregroundStyle(.secondary)
                        HStack {
                            if item["action"] is [String: Any] {
                                Button("Salva la procedura", systemImage: "square.and.arrow.down") {
                                    mutate("/api/improvements/apply", body: ["id": id])
                                }.buttonStyle(.borderedProminent)
                            }
                            Button("Nascondi") { mutate("/api/improvements/dismiss", body: ["id": id]) }
                        }
                    }
                }
            }
        }
    }

    /// Piano proposto dal modello (ADR-0160): crea una bozza da rivedere, non la avvia.
    private var planProposal: some View {
        card("Fai proporre un piano al modello", icon: "wand.and.stars") {
            Text("Descrivi l'obiettivo: il modello propone i passi, il Core li valida e ne mostra il rischio. La bozza non parte da sola.")
                .font(.callout).foregroundStyle(.secondary)
            TextField("Es. Riassumi lo stato del repository e controlla le dipendenze", text: $planGoal, axis: .vertical)
                .textFieldStyle(.roundedBorder).lineLimit(1...3)
            Button("Proponi una bozza", systemImage: "sparkles") {
                let goal = planGoal.trimmingCharacters(in: .whitespacesAndNewlines)
                planGoal = ""
                mutate("/api/plans/propose", body: ["goal": String(goal.prefix(300)), "create": true])
            }
            .disabled(planGoal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
    }

    /// Arresto di emergenza (ADR-0151): ferma ogni modifica, i lavori in background e le automazioni.
    private var emergencyCard: some View {
        let active = payload["emergency_stop"] as? Bool ?? false
        return card("Arresto di emergenza", icon: "power") {
            Text(active
                 ? "Attivo: JARVIS non esegue né modifica nulla e le attività autonome sono ferme."
                 : "Ferma subito ogni modifica, i lavori in background, le missioni e le automazioni. Si disattiva da qui.")
                .font(.callout).foregroundStyle(active ? .red : .secondary)
            Button(active ? "Disattiva l'arresto" : "Arresto di emergenza", systemImage: "power") {
                mutate("/api/emergency", body: ["active": !active])
            }
            .buttonStyle(.borderedProminent).tint(active ? .green : .red)
        }
    }

    private var missionTools: some View {
        VStack(spacing: 14) {
            card("Nuova missione", icon: "scope") {
                Text("JARVIS la porta avanti da solo in background, con strumenti che non chiedono conferma, e ti manda un rapporto a ogni esecuzione.")
                    .font(.callout).foregroundStyle(.secondary)
                if payload["cloud"] as? Bool != true {
                    Label("Nessun modello cloud configurato: userà solo i comandi locali.", systemImage: "exclamationmark.triangle")
                        .font(.callout).foregroundStyle(.orange)
                }
                TextField("Es. Ogni mattina controlla le novità su Rust e riassumile", text: $missionGoal, axis: .vertical)
                    .textFieldStyle(.roundedBorder).lineLimit(2...4)
                HStack {
                    TextField("Ripeti ogni (minuti)", text: $missionEvery).textFieldStyle(.roundedBorder).frame(width: 190)
                    Toggle("Può usare i connettori", isOn: $missionConnectors)
                    Spacer()
                    Button("Avvia missione") {
                        var body: [String: Any] = ["goal": missionGoal.trimmingCharacters(in: .whitespacesAndNewlines),
                                                   "connectors": missionConnectors]
                        if let minutes = Int(missionEvery.trimmingCharacters(in: .whitespaces)) { body["every_minutes"] = minutes }
                        missionGoal = ""
                        missionEvery = ""
                        mutate("/api/missions", body: body)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(missionGoal.trimmingCharacters(in: .whitespacesAndNewlines).count < 8
                              || (!missionEvery.trimmingCharacters(in: .whitespaces).isEmpty
                                  && Int(missionEvery.trimmingCharacters(in: .whitespaces)) == nil))
                }
            }
            card("Iniziative proattive", icon: "sparkles") {
                Text("JARVIS si fa avanti da solo, con regole locali: nulla esce dal Core. Nelle ore di silenzio non invia nulla.")
                    .font(.callout).foregroundStyle(.secondary)
                let settings = payload["proactive"] as? [String: Any] ?? [:]
                let toggles: [(key: String, label: String)] = [
                    ("enabled", "Iniziative attive"), ("briefing", "Briefing quotidiano"),
                    ("weekly_briefing", "Briefing settimanale della direzione (lunedì)"),
                    ("disk", "Avvisi sullo spazio disco"), ("confirmations", "Promemoria delle conferme in attesa"),
                    ("workflows", "Automazioni che continuano a fallire"), ("stale_todos", "Cose da fare lasciate indietro"), ("sandbox", "Sandbox quasi piena")
                ]
                ForEach(toggles.indices, id: \.self) { index in
                    let entry = toggles[index]
                    Toggle(entry.label, isOn: Binding(
                        get: { settings[entry.key] as? Bool ?? false },
                        set: { mutate("/api/proactive", method: "PATCH", body: [entry.key: $0]) }
                    ))
                }
                Text("Briefing alle \(settings["briefing_time"] as? String ?? "08:00"), silenzio dalle \(settings["quiet_from"] as? String ?? "22:00") alle \(settings["quiet_to"] as? String ?? "07:00").")
                    .font(.footnote).foregroundStyle(.secondary)
                HStack {
                    Button("Briefing adesso") { mutate("/api/proactive/briefing", body: [:]) }
                    Button("Briefing settimanale adesso") { mutate("/api/proactive/weekly", body: [:]) }
                }
            }
        }
    }

    private var deviceConnection: some View {
        VStack(spacing: 14) {
            card("Connessione HTTPS", icon: "network") {
                let origins = payload["pairing_urls"] as? [String] ?? []
                let origin = origins.contains(remoteOrigin) ? remoteOrigin : origins.first ?? ""
                if origins.isEmpty {
                    Button("Diagnostica Tailscale", systemImage: "stethoscope") { diagnoseTailscale() }
                    Button("Collega con Tailscale", systemImage: "point.3.connected.trianglepath.dotted") {
                        mutate("/api/tailscale/connect", body: [:])
                    }.buttonStyle(.borderedProminent)
                    Text("Con Tailscale installato e connesso basta un tocco: JARVIS diventa raggiungibile solo dai tuoi dispositivi (mai pubblico) e l'indirizzo si autorizza da solo.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                HStack {
                    TextField("Indirizzo pubblico (es. casa.tailnet.ts.net)", text: $publicHost)
                        .textFieldStyle(.roundedBorder).onSubmit(addPublicHost)
                    Button("Aggiungi", systemImage: "plus", action: addPublicHost)
                        .disabled(publicHost.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                if origins.isEmpty {
                    Text("Inserisci qui l'indirizzo HTTPS con cui il Core è raggiungibile (per esempio quello di Tailscale): si salva da solo, senza file da modificare né riavvii.")
                        .font(.callout).foregroundStyle(.secondary)
                } else {
                    Picker("Indirizzo del Core", selection: Binding(
                        get: { origin }, set: { remoteOrigin = $0 }
                    )) {
                        ForEach(origins, id: \.self) { Text($0).tag($0) }
                    }
                    HStack {
                        Button("Verifica HTTPS", systemImage: "checkmark.shield") { probe(origin) }
                            .buttonStyle(.borderedProminent).disabled(checkingHTTPS)
                        Button("Copia indirizzo", systemImage: "doc.on.doc") { copyConnectionText(origin) }
                        Button("Rimuovi", systemImage: "trash", role: .destructive) {
                            mutate("/api/devices/access", method: "PUT", body: ["remove": origin.replacingOccurrences(of: "https://", with: "")])
                        }
                    }
                    if checkingHTTPS {
                        ProgressView("Verifica del certificato e del Core").controlSize(.small)
                    } else if let result = connectionProbe, result.origin == origin {
                        Label(result.detail, systemImage: result.verified ? "checkmark.seal.fill" : "exclamationmark.triangle")
                            .font(.callout).foregroundStyle(result.verified ? Color.green : .orange)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            card("Associa dispositivo", icon: "qrcode") {
                Text("Il dispositivo riceve accesso alla chat.")
                    .foregroundStyle(.secondary)
                Toggle("Consenti anche le conferme", isOn: $allowDeviceConfirmations)
                    .disabled(changingInvite)
                HStack {
                    Button(invite == nil ? "Genera codice" : "Nuovo codice", systemImage: "key") {
                        updateInvite(cancel: false)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(changingInvite || (payload["pairing_urls"] as? [String] ?? []).isEmpty)
                    if invite != nil {
                        Button("Annulla invito", role: .destructive) { updateInvite(cancel: true) }
                            .disabled(changingInvite)
                    }
                }
                if let code = invite?["code"] as? String, let date = inviteExpiry {
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        if context.date < date {
                            VStack(alignment: .leading, spacing: 8) {
                                HStack {
                                    Text(code).font(.title2.monospaced()).tracking(3).textSelection(.enabled)
                                    Spacer()
                                    Button("Copia codice", systemImage: "doc.on.doc") { copyConnectionText(code) }
                                }
                                if let link = invite?["link"] as? String, let url = URL(string: link) {
                                    HStack {
                                        ShareLink(item: url) { Label("Condividi collegamento", systemImage: "square.and.arrow.up") }
                                        Button("Copia collegamento", systemImage: "link") { copyConnectionText(link) }
                                    }
                                    Text("Aprilo sul telefono: indirizzo e codice si compilano da soli.")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                LabeledContent("Scadenza") { Text(date, style: .time) }
                                LabeledContent("Permessi", value:
                                    (invite?["scopes"] as? [String])?.contains("confirm") == true ? "Chat e conferme" : "Chat")
                            }
                            .padding(12)
                            .background(.cyan.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
                        } else {
                            Label("Codice scaduto", systemImage: "exclamationmark.circle")
                                .foregroundStyle(.orange)
                        }
                    }
                }
            }
        }
    }

    private func applyProfile(_ doc: [String: Any]) {
        var draft: [String: String] = [:]
        for key in ["role", "company", "sector", "expertise", "interests", "goals", "language", "tone", "level"] {
            draft[key] = doc[key] as? String ?? ""
        }
        profileDraft = draft
        profileShare = doc["share_with_cloud"] as? Bool ?? true
    }

    private func profileField(_ title: String, _ key: String, _ prompt: String) -> some View {
        TextField(title, text: Binding(get: { profileDraft[key] ?? "" }, set: { profileDraft[key] = $0 }), prompt: Text(prompt))
            .textFieldStyle(.roundedBorder)
    }

    private func profileChoice(_ title: String, _ key: String, labels: [String: String]) -> some View {
        let choices = (payload["choices"] as? [String: Any])?[key] as? [String] ?? ["auto"]
        return Picker(title, selection: Binding(get: { profileDraft[key] ?? "auto" }, set: { profileDraft[key] = $0 })) {
            ForEach(choices, id: \.self) { Text(labels[$0] ?? $0.capitalized).tag($0) }
        }
    }

    /// Il profilo aiuta JARVIS a rispondere nel tuo contesto (tono, livello, lavoro): niente recapiti, tutto facoltativo.
    private var profileEditor: some View {
        VStack(spacing: 14) {
            card("Il tuo profilo", icon: "person.crop.circle") {
                let percent = payload["completion_percent"] as? Int ?? 0
                HStack {
                    ProgressView(value: Double(percent), total: 100).tint(.cyan)
                    Text("\(percent)%").font(.callout.monospacedDigit()).foregroundStyle(.secondary)
                }
                if let missing = payload["missing"] as? [String], !missing.isEmpty {
                    Text("Per risposte più mirate aggiungi: " + missing.prefix(3).joined(separator: ", ") + ".")
                        .font(.caption).foregroundStyle(.secondary)
                }
                profileField("Ruolo", "role", "Direttore operativo")
                profileField("Azienda", "company", "Nome dell'azienda")
                profileField("Settore", "sector", "Manifattura, sanità, software…")
                profileField("Competenze", "expertise", "Cosa conosci bene")
                profileField("Interessi", "interests", "Argomenti che ti interessano")
                profileField("Obiettivi", "goals", "Cosa vuoi ottenere")
            }
            card("Come preferisci le risposte", icon: "text.bubble") {
                profileChoice("Lingua", "language", labels: ["auto": "Automatica", "it": "Italiano", "en": "Inglese"])
                profileChoice("Tono", "tone", labels: ["auto": "Automatico", "formale": "Formale", "cordiale": "Cordiale", "diretto": "Diretto"])
                profileChoice("Livello tecnico", "level", labels: ["auto": "Automatico", "principiante": "Principiante", "intermedio": "Intermedio", "esperto": "Esperto"])
                Toggle("Usa il profilo nelle richieste ai modelli cloud", isOn: $profileShare)
                Text("Nel profilo non vanno email, telefoni o password: JARVIS li rifiuta. Se disattivi l'opzione, il profilo resta qui e non esce dal computer.")
                    .font(.caption).foregroundStyle(.secondary)
                Button("Salva profilo", systemImage: "checkmark") {
                    var body: [String: Any] = profileDraft.reduce(into: [:]) { $0[$1.key] = $1.value }
                    body["share_with_cloud"] = profileShare
                    mutate("/api/profile", method: "PATCH", body: body)
                }.buttonStyle(.borderedProminent)
            }
        }
    }

    /// Diagnosi del collegamento da questo Mac (DNS, porta, certificato, risposta di JARVIS): dice quale passo si ferma.
    private func diagnoseTailscale() {
        error = nil
        info = "Diagnosi di Tailscale in corso…"
        Task {
            do {
                let result = try await core.request("/api/tailscale/diagnose", method: "POST", body: [:], timeout: 30)
                info = result["message"] as? String ?? "Diagnosi completata."
            } catch {
                info = nil
                self.error = error.localizedDescription
            }
        }
    }

    // MARK: Centro privacy (ADR-0187)

    private func bytesText(_ n: Int) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(n), countStyle: .file)
    }

    /// Cosa JARVIS conserva, dove, cosa può uscire verso il cloud e cosa è attivo: legge `/api/privacy/overview`.
    private var privacyPanel: some View {
        let stored = payload["stored_locally"] as? [String: Any] ?? [:]
        let entries = (stored["data_dir_entries"] as? [[String: Any]] ?? [])
            .sorted { ($0["bytes"] as? Int ?? 0) > ($1["bytes"] as? Int ?? 0) }
        let memory = stored["memory"] as? [String: Any] ?? [:]
        let cloud = payload["cloud"] as? [String: Any] ?? [:]
        let providers = cloud["providers_with_key"] as? [[String: Any]] ?? []
        let tools = payload["tools"] as? [String: Any] ?? [:]
        let byEffects = (tools["by_effects"] as? [String: Any] ?? [:]).sorted { $0.key < $1.key }
        let autonomy = payload["autonomy"] as? [String: Any] ?? [:]
        let background = payload["background"] as? [String: Any] ?? [:]
        return VStack(alignment: .leading, spacing: 14) {
            card("Dati conservati sul computer", icon: "internaldrive") {
                Text("Memoria: \(memory["total"] as? Int ?? 0) ricordi. \(stored["note"] as? String ?? "")")
                    .font(.callout).foregroundStyle(.secondary)
                ForEach(Array(entries.prefix(8).enumerated()), id: \.offset) { _, entry in
                    HStack {
                        Text(entry["name"] as? String ?? "?").font(.system(.callout, design: .monospaced))
                        Spacer()
                        Text(bytesText(entry["bytes"] as? Int ?? 0)).font(.caption).foregroundStyle(.secondary)
                    }
                }
                HStack {
                    Button("Gestisci la memoria") { select(.memory) }
                    Button("Esporta o elimina dati") { select(.system) }
                }
            }
            card("Cosa può uscire verso il cloud", icon: "icloud.and.arrow.up") {
                Text(cloud["note"] as? String ?? "").font(.callout).foregroundStyle(.secondary)
                if providers.isEmpty {
                    Label("Nessun provider cloud ha una chiave: nulla parte dal computer.", systemImage: "checkmark.shield")
                        .foregroundStyle(.green)
                } else {
                    ForEach(Array(providers.enumerated()), id: \.offset) { _, p in
                        Label("\(p["id"] as? String ?? "provider"): chiave configurata", systemImage: "key.fill")
                    }
                }
                Button("Modelli e chiavi") { select(.models) }
            }
            card("Cosa è autorizzato", icon: "wrench.and.screwdriver") {
                Text("Strumenti disponibili: \(tools["count"] as? Int ?? 0). Autonomia: livello \(autonomy["default"] as? Int ?? 0); lavori in background \(autonomy["background_allowed"] as? Bool == true ? "consentiti" : "non consentiti").")
                    .font(.callout)
                ForEach(byEffects, id: \.key) { effect in
                    HStack { Text(effect.key); Spacer(); Text("\(effect.value as? Int ?? 0)").monospacedDigit().foregroundStyle(.secondary) }
                        .font(.caption)
                }
                HStack {
                    Text("Connettori: \(payload["connectors"] as? Int ?? 0) · Server MCP: \(payload["mcp_servers"] as? Int ?? 0) · Osservazioni di file: \(background["file_watches"] as? Int ?? 0) · Profilo risorse: \(background["resource_profile"] as? String ?? "?")")
                        .font(.caption).foregroundStyle(.secondary)
                }
                HStack {
                    Button("Permessi e autonomia") { select(.security) }
                    Button("Connettori") { select(.connectors) }
                    Button("Audit") { select(.audit) }
                }
            }
            card("Cronologia dei permessi", icon: "clock.arrow.circlepath") {
                let history = (payload["permission_history"] as? [String: Any])?["items"] as? [[String: Any]] ?? []
                if history.isEmpty {
                    Text("Nessuna modifica registrata.").font(.callout).foregroundStyle(.secondary)
                }
                ForEach(Array(history.prefix(10).enumerated()), id: \.offset) { _, entry in
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(entry["at"] as? String ?? "?") · \(entry["kind"] as? String ?? "?")")
                            .font(.caption.weight(.semibold))
                        ForEach(Array((entry["changes"] as? [[String: Any]] ?? []).prefix(6).enumerated()), id: \.offset) { _, change in
                            Text("\(change["field"] as? String ?? "?"): \(String(describing: change["from"] ?? "—")) → \(String(describing: change["to"] ?? "—"))")
                                .font(.system(.caption, design: .monospaced)).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }

    // MARK: Dataset (ADR-0221)

    private func evaluateDataset(_ name: String) {
        error = nil
        Task {
            do {
                let r = try await core.request("/api/datasets/\(name)/evaluate")
                let correct = r["correct"] as? Int ?? 0, total = r["total"] as? Int ?? 0
                var text = "\(name): \(correct) su \(total) corretti (\(r["accuracy_percent"] as? Double ?? 0)%)."
                if let support = r["support"] as? [String: Any] {
                    text += " Supporto: \(support["hit"] as? Int ?? 0) su \(support["total"] as? Int ?? 0)."
                }
                datasetReport = text + "\n" + (r["note"] as? String ?? "")
            } catch { self.error = error.localizedDescription }
        }
    }

    private func pushDataset(_ name: String) {
        let repo = hfRepo.trimmingCharacters(in: .whitespacesAndNewlines)
        error = nil
        info = nil
        pendingDatasetPush = nil
        Task {
            do {
                let r = try await core.request("/api/datasets/\(name)/push", method: "POST",
                                               body: ["repo_id": repo, "confirm": true], timeout: 120)
                let isPrivate = r["private"] as? Bool == true
                info = "Caricato in \(r["repo"] as? String ?? repo) (\(isPrivate ? "privato" : "ATTENZIONE: non risulta privato"))."
                celebrateTick += 1
                await load(section)
            } catch { self.error = error.localizedDescription }
        }
    }

    /// Dataset di JARVIS: elenco, misura e caricamento su Hugging Face, sempre in un repository privato.
    private var datasetsPanel: some View {
        let items = payload["items"] as? [[String: Any]] ?? []
        return VStack(alignment: .leading, spacing: 14) {
            card("Dataset di JARVIS", icon: "tablecells") {
                Text("Esempi con cui JARVIS capisce meglio le tue richieste. Restano sul computer: li carichi su Hugging Face solo tu, in un repository privato.")
                    .font(.callout).foregroundStyle(.secondary)
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    let name = item["name"] as? String ?? ""
                    VStack(alignment: .leading, spacing: 4) {
                        Text(name).font(.headline)
                        Text("versione \(item["version"] as? Int ?? 1) · \(item["examples"] as? Int ?? 0) esempi · \(item["candidates"] as? Int ?? 0) candidati in attesa")
                            .font(.caption).foregroundStyle(.secondary)
                        if let pushed = item["pushed"] as? [String: Any], let repo = pushed["repo"] as? String {
                            Label("Caricato in \(repo)", systemImage: "lock.icloud").font(.caption).foregroundStyle(.green)
                        }
                        Button("Misura") { evaluateDataset(name) }
                    }
                }
            }
            if let report = datasetReport {
                card("Risultato della misura", icon: "checkmark.seal") {
                    Text(report).font(.callout).textSelection(.enabled)
                    Button("Chiudi") { datasetReport = nil }
                }
            }
            card("Carica su Hugging Face (sempre privato)", icon: "icloud.and.arrow.up") {
                Text("Serve il connettore Hugging Face con un token che possa scrivere sul repository. JARVIS crea il repository privato se manca e rifiuta quelli pubblici. Il server MCP di Hugging Face è in sola lettura: serve a rileggere i dataset, non a scriverli.")
                    .font(.callout).foregroundStyle(.secondary)
                TextField("proprietario/nome, per esempio mario/jarvis-routing", text: $hfRepo)
                    .textFieldStyle(.roundedBorder)
                if let name = pendingDatasetPush {
                    Text("Confermi il caricamento di «\(name)» in «\(hfRepo)» (privato)?").font(.callout.weight(.semibold))
                    HStack {
                        Button("Conferma e carica") { pushDataset(name) }.buttonStyle(.borderedProminent)
                        Button("Annulla") { pendingDatasetPush = nil }
                    }
                } else {
                    ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                        let name = item["name"] as? String ?? ""
                        Button("Carica \(name)") { pendingDatasetPush = name }
                            .disabled(!hfRepo.contains("/"))
                    }
                }
                Button("Apri il connettore Hugging Face") { select(.connectors) }
            }
        }
    }

    // MARK: Ricerche salvate (ADR-0206)

    private func openResearch(_ id: String) {
        guard !id.isEmpty else { return }
        error = nil
        Task {
            do {
                let result = try await core.request("/api/research/\(id)")
                researchText = result["markdown"] as? String ?? "Ricerca vuota."
            } catch { self.error = error.localizedDescription }
        }
    }

    /// Ricerche salvate dal proprietario: elenco, lettura del rapporto ed eliminazione. Si creano dalla chat.
    private var researchPanel: some View {
        let items = payload["items"] as? [[String: Any]] ?? []
        return VStack(alignment: .leading, spacing: 14) {
            card("Ricerche salvate", icon: "magnifyingglass.circle") {
                Text("Si creano dalla chat («salva questa ricerca come …»). Qui puoi rileggerle o eliminarle.")
                    .font(.callout).foregroundStyle(.secondary)
                if items.isEmpty {
                    Text("Nessuna ricerca salvata.").foregroundStyle(.secondary)
                }
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    let id = item["id"] as? String ?? ""
                    VStack(alignment: .leading, spacing: 4) {
                        Text(item["title"] as? String ?? "Ricerca").font(.headline)
                        Text("\(item["findings"] as? Int ?? 0) risultati · \(item["sources"] as? Int ?? 0) fonti · \(item["open_questions"] as? Int ?? 0) domande aperte · \(item["status"] as? String ?? "")")
                            .font(.caption).foregroundStyle(.secondary)
                        HStack {
                            Button("Apri") { openResearch(id) }
                            Button("Elimina", role: .destructive) {
                                researchText = nil
                                mutate("/api/research/\(id)", method: "DELETE", body: [:])
                            }
                        }
                    }
                }
            }
            if let text = researchText {
                card("Rapporto", icon: "doc.text") {
                    Text(text).font(.callout).textSelection(.enabled)
                    Button("Chiudi") { researchText = nil }
                }
            }
        }
    }

    // MARK: Mappa (ADR-0202)

    private func coordinate(_ value: Any?) -> CLLocationCoordinate2D? {
        guard let pair = value as? [Any], pair.count == 2,
              let lat = (pair[0] as? NSNumber)?.doubleValue, let lon = (pair[1] as? NSNumber)?.doubleValue else { return nil }
        return CLLocationCoordinate2D(latitude: lat, longitude: lon)
    }

    private func fit(_ points: [CLLocationCoordinate2D]) {
        guard let first = points.first else { return }
        var (minLat, maxLat, minLon, maxLon) = (first.latitude, first.latitude, first.longitude, first.longitude)
        for p in points {
            minLat = min(minLat, p.latitude); maxLat = max(maxLat, p.latitude)
            minLon = min(minLon, p.longitude); maxLon = max(maxLon, p.longitude)
        }
        let region = MKCoordinateRegion(
            center: CLLocationCoordinate2D(latitude: (minLat + maxLat) / 2, longitude: (minLon + maxLon) / 2),
            span: MKCoordinateSpan(latitudeDelta: max((maxLat - minLat) * 1.4, 0.02), longitudeDelta: max((maxLon - minLon) * 1.4, 0.02)))
        mapCamera = .region(region)
    }

    private func searchMap() {
        let query = mapQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty, !mapBusy else { return }
        mapBusy = true
        error = nil
        Task {
            defer { mapBusy = false }
            do {
                let result = try await core.request("/api/map/search", method: "POST", body: ["query": query, "limit": 5], timeout: 40)
                let places = result["places"] as? [[String: Any]] ?? []
                mapSpots = places.compactMap { p in
                    guard let lat = (p["lat"] as? NSNumber)?.doubleValue, let lon = (p["lon"] as? NSNumber)?.doubleValue else { return nil }
                    return MapSpot(name: p["name"] as? String ?? query, coordinate: CLLocationCoordinate2D(latitude: lat, longitude: lon))
                }
                mapPath = []
                mapSteps = []
                mapSummary = places.isEmpty ? "Nessun luogo trovato per «\(query)»."
                    : (places.first?["display_name"] as? String ?? query)
                mapLink = (places.first?["map_url"] as? String).flatMap(URL.init(string:))
                fit(mapSpots.map(\.coordinate))
            } catch { self.error = error.localizedDescription }
        }
    }

    private func routeMap() {
        let from = mapFrom.trimmingCharacters(in: .whitespacesAndNewlines)
        let to = mapTo.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !from.isEmpty, !to.isEmpty, !mapBusy else { return }
        mapBusy = true
        error = nil
        Task {
            defer { mapBusy = false }
            do {
                let result = try await core.request("/api/map/route", method: "POST",
                                                    body: ["from": from, "to": to, "mode": mapMode], timeout: 60)
                func spot(_ key: String) -> MapSpot? {
                    guard let p = result[key] as? [String: Any],
                          let lat = (p["lat"] as? NSNumber)?.doubleValue, let lon = (p["lon"] as? NSNumber)?.doubleValue else { return nil }
                    return MapSpot(name: p["name"] as? String ?? key, coordinate: CLLocationCoordinate2D(latitude: lat, longitude: lon))
                }
                mapSpots = [spot("from"), spot("to")].compactMap { $0 }
                mapPath = (result["path"] as? [Any] ?? []).compactMap { coordinate($0) }
                mapSteps = (result["steps"] as? [[String: Any]] ?? []).compactMap { $0["instruction"] as? String }
                mapSummary = result["summary"] as? String ?? ""
                mapLink = (result["map_url"] as? String).flatMap(URL.init(string:))
                fit(mapPath.isEmpty ? mapSpots.map(\.coordinate) : mapPath)
            } catch { self.error = error.localizedDescription }
        }
    }

    /// Mappa di OpenStreetMap con ricerca di luoghi e percorsi. Fuori dal Mac vanno solo i nomi cercati: JARVIS non
    /// conosce la tua posizione. Le stesse funzioni sono disponibili in chat («indicazioni da Torino a Milano»).
    private var mapView: some View {
        VStack(alignment: .leading, spacing: 14) {
            card("Mappa", icon: "map") {
                Text("Cerca un luogo o calcola un percorso con dati OpenStreetMap. Nella chat puoi chiedere anche «indicazioni da Torino a Milano a piedi» o «mostrami Firenze sulla mappa».")
                    .font(.callout).foregroundStyle(.secondary)
                HStack {
                    TextField("Cerca un luogo (per esempio Colosseo, Roma)", text: $mapQuery)
                        .textFieldStyle(.roundedBorder).onSubmit { searchMap() }
                    Button("Cerca", systemImage: "magnifyingglass") { searchMap() }.disabled(mapBusy)
                }
                Map(position: $mapCamera) {
                    ForEach(mapSpots) { spot in Marker(spot.name, coordinate: spot.coordinate) }
                    if mapPath.count > 1 { MapPolyline(coordinates: mapPath).stroke(Color.cyan, lineWidth: 5) }
                }
                .frame(height: 340)
                .clipShape(RoundedRectangle(cornerRadius: 12))
                if !mapSummary.isEmpty { Text(mapSummary).font(.callout) }
                if let link = mapLink { Link("Apri su OpenStreetMap", destination: link).font(.caption) }
            }
            card("Percorso", icon: "arrow.triangle.turn.up.right.diamond") {
                HStack {
                    TextField("Da", text: $mapFrom).textFieldStyle(.roundedBorder)
                    TextField("A", text: $mapTo).textFieldStyle(.roundedBorder)
                }
                HStack {
                    Picker("Come", selection: $mapMode) {
                        Text("In auto").tag("auto")
                        Text("A piedi").tag("piedi")
                        Text("In bici").tag("bici")
                    }.pickerStyle(.segmented).frame(maxWidth: 280)
                    Spacer()
                    if mapBusy { ProgressView().controlSize(.small) }
                    Button("Calcola percorso", systemImage: "location.north.line") { routeMap() }
                        .disabled(mapBusy || mapFrom.isEmpty || mapTo.isEmpty)
                }
                ForEach(Array(mapSteps.enumerated()), id: \.offset) { index, step in
                    Text("\(index + 1). \(step)").font(.callout)
                }
                if !mapSteps.isEmpty {
                    Text("Percorso indicativo, senza traffico in tempo reale.").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }

    // MARK: Installazione della voce (ADR-0200)

    private func loadVoiceStatus() async {
        if let result = try? await core.request("/api/setup", timeout: 15) { voiceStatus = result }
    }

    private func startVoiceInstall() {
        error = nil
        info = nil
        Task {
            do {
                _ = try await core.request("/api/setup/start", method: "POST",
                                           body: ["stt_model": voiceStt, "tts_voice": voiceTts], timeout: 30)
                info = "Installazione della voce avviata."
                await loadVoiceStatus()
            } catch { self.error = error.localizedDescription }
        }
    }

    /// Sezione dedicata alla voce: la voce di sistema del Mac non richiede nulla; il motore locale del Core
    /// (riconoscimento e sintesi offline, parola di attivazione) si installa da qui, con dimensioni e avanzamento.
    private var voiceInstaller: some View {
        let running = voiceStatus["running"] as? Bool ?? false
        let sizes = voiceStatus["sizes_mb"] as? [String: Any] ?? [:]
        let sttSizes = sizes["stt"] as? [String: Any] ?? [:]
        let choices = voiceStatus["choices"] as? [String: Any] ?? [:]
        let sttChoices = choices["stt_models"] as? [String] ?? ["tiny", "base", "small", "medium"]
        let ttsChoices = choices["tts_voices"] as? [String] ?? ["it_IT-riccardo-x_low", "en_US-ryan-medium"]
        let packages = sizes["voice_packages"] as? Int ?? 0
        let stt = sttSizes[voiceStt] as? Int ?? 0
        let tts = sizes["tts"] as? Int ?? 0
        let steps = voiceStatus["steps"] as? [[String: Any]] ?? []
        let installed = (voiceStatus["detected"] as? [String: Any])?["voice_packages"] as? Bool == true
        let available = voiceStatus["available"] as? Bool ?? true
        return VStack(alignment: .leading, spacing: 14) {
            card("Voce di sistema del Mac", icon: "speaker.wave.2") {
                Text("Lettura delle risposte e ascolto di «Hey Jarvis» usano le voci e il riconoscimento vocale di macOS: non c'è nulla da installare. Voci più naturali si scaricano da Impostazioni di sistema → Accessibilità → Contenuto letto → Voce di sistema → Gestisci voci.")
                    .font(.callout).foregroundStyle(.secondary)
                Label(wake.engineKind == "system" ? "In uso: riconoscimento del Mac" : "In uso: motore del Core",
                      systemImage: "checkmark.circle").font(.caption).foregroundStyle(.green)
            }
            card("Motore vocale locale del Core", icon: "arrow.down.circle") {
                Text("Facoltativo: installa sul Mac il riconoscimento vocale (faster-whisper), la voce di sintesi (Piper) e la parola di attivazione, così la voce funziona anche senza i servizi di macOS e senza rete. Serve una connessione per scaricare i file, una sola volta.")
                    .font(.callout).foregroundStyle(.secondary)
                if !available {
                    Label("L'installazione non è abilitata su questo Core.", systemImage: "exclamationmark.triangle")
                        .font(.callout).foregroundStyle(.orange)
                }
                Label(installed ? "Pacchetti vocali già installati" : "Non ancora installato",
                      systemImage: installed ? "checkmark.seal.fill" : "tray.and.arrow.down")
                    .foregroundStyle(installed ? Color.green : .secondary)
                Picker("Riconoscimento (STT)", selection: $voiceStt) {
                    ForEach(sttChoices, id: \.self) { Text("\($0) · \((sttSizes[$0] as? Int) ?? 0) MB").tag($0) }
                }
                Picker("Voce di sintesi (TTS)", selection: $voiceTts) {
                    ForEach(ttsChoices, id: \.self) { Text($0).tag($0) }
                }
                Text("Download stimato: circa \(packages + stt + tts) MB. Resta nella cartella dati di JARVIS.")
                    .font(.caption).foregroundStyle(.secondary)
                Button(installed ? "Reinstalla o cambia modello" : "Installa la voce", systemImage: "arrow.down.circle.fill") {
                    voiceConfirm = true
                }
                .disabled(running || !available)
                .buttonStyle(.borderedProminent)
                ForEach(Array(steps.enumerated()), id: \.offset) { _, step in
                    let status = step["status"] as? String ?? "pending"
                    HStack(alignment: .top) {
                        Image(systemName: status == "done" ? "checkmark.circle.fill" : status == "running" ? "arrow.triangle.2.circlepath"
                              : status == "failed" ? "xmark.octagon.fill" : "circle.dotted")
                            .foregroundStyle(status == "done" ? Color.green : status == "failed" ? .red : .cyan)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(step["title"] as? String ?? step["label"] as? String ?? step["id"] as? String ?? "")
                            if let detail = step["detail"] as? String, !detail.isEmpty {
                                Text(detail).font(.caption).foregroundStyle(.secondary)
                            }
                            if status == "running", let progress = step["progress"] as? Double {
                                ProgressView(value: min(max(progress / 100, 0), 1))
                            }
                        }
                    }
                }
            }
        }
        .alert("Installare la voce?", isPresented: $voiceConfirm) {
            Button("Installa") { startVoiceInstall() }
            Button("Annulla", role: .cancel) {}
        } message: {
            Text("Verranno scaricati circa \(packages + stt + tts) MB (pacchetti Python, modello «\(voiceStt)» e voce «\(voiceTts)»). Puoi continuare a usare JARVIS durante il download.")
        }
        .task {
            while !Task.isCancelled && section == .voice {
                await loadVoiceStatus()
                try? await Task.sleep(for: .seconds(running ? 2 : 6))
            }
        }
    }

    // MARK: Profili di workspace (ADR-0185)

    private func loadWorkspaceProfiles() async {
        if let result = try? await core.request("/api/workspaces/profiles", timeout: 20) {
            wsProfiles = result["items"] as? [[String: Any]] ?? []
        }
    }

    private func wsList(_ text: String) -> [String] {
        text.split(whereSeparator: { $0 == "," || $0 == "\n" })
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
    }

    private func workspaceCall(_ path: String, body: [String: Any]) {
        error = nil
        info = nil
        Task {
            do {
                let result = try await core.request(path, method: "POST", body: body, timeout: 60)
                info = result["message"] as? String ?? "Fatto."
                await loadWorkspaceProfiles()
            } catch { self.error = error.localizedDescription }
        }
    }

    /// Profili di lavoro scritti a mano: un nome, le app da aprire e le cartelle della workspace. Attivarli apre
    /// soltanto le app elencate, con il tuo tocco come conferma; non lancia comandi.
    private var workspaceProfilesCard: some View {
        card("Profili di lavoro", icon: "rectangle.3.group") {
            Text("Crea a mano un ambiente di lavoro: dai un nome, elenca le app da aprire e le cartelle della workspace. «Attiva» apre le app. Nessun comando né indirizzo: solo nomi di app e cartelle.")
                .font(.callout).foregroundStyle(.secondary)
            ForEach(Array(wsProfiles.enumerated()), id: \.offset) { _, profile in
                let name = profile["name"] as? String ?? "Profilo"
                let apps = profile["apps"] as? [String] ?? []
                let dirs = profile["dirs"] as? [String] ?? []
                Divider()
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(name).font(.headline)
                        if let text = profile["description"] as? String, !text.isEmpty {
                            Text(text).font(.caption).foregroundStyle(.secondary)
                        }
                        if !apps.isEmpty { Label(apps.joined(separator: ", "), systemImage: "app.badge").font(.caption) }
                        if !dirs.isEmpty { Label(dirs.joined(separator: ", "), systemImage: "folder").font(.caption) }
                    }
                    Spacer()
                    Button("Attiva", systemImage: "play.fill") {
                        workspaceCall("/api/workspaces/profiles/activate", body: ["id": profile["id"] as? String ?? name])
                    }.disabled(apps.isEmpty)
                    Button("Modifica") {
                        wsName = name
                        wsDescription = profile["description"] as? String ?? ""
                        wsApps = apps.joined(separator: ", ")
                        wsDirs = dirs.joined(separator: ", ")
                    }
                    Button("Elimina", role: .destructive) {
                        workspaceCall("/api/workspaces/profiles/remove", body: ["id": profile["id"] as? String ?? name])
                    }
                }
            }
            Divider()
            Text(wsProfiles.contains { ($0["name"] as? String)?.lowercased() == wsName.lowercased() && !wsName.isEmpty } ? "Modifica profilo" : "Nuovo profilo")
                .font(.subheadline.weight(.semibold))
            TextField("Nome (per esempio Sviluppo JARVIS)", text: $wsName).textFieldStyle(.roundedBorder)
            TextField("Descrizione (facoltativa)", text: $wsDescription).textFieldStyle(.roundedBorder)
            TextField("App da aprire, separate da virgola (per esempio Terminale, Visual Studio Code, Safari)", text: $wsApps)
                .textFieldStyle(.roundedBorder)
            TextField("Cartelle della workspace, separate da virgola (per esempio progetti/jarvis)", text: $wsDirs)
                .textFieldStyle(.roundedBorder)
            HStack {
                Button("Salva profilo", systemImage: "square.and.arrow.down") {
                    let body: [String: Any] = ["name": wsName.trimmingCharacters(in: .whitespacesAndNewlines),
                                               "description": wsDescription, "apps": wsList(wsApps), "dirs": wsList(wsDirs)]
                    wsName = ""; wsDescription = ""; wsApps = ""; wsDirs = ""
                    workspaceCall("/api/workspaces/profiles", body: body)
                }
                .disabled(wsName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button("Svuota") { wsName = ""; wsDescription = ""; wsApps = ""; wsDirs = "" }
            }
        }
        .task { await loadWorkspaceProfiles() }
    }

    // MARK: Server MCP e Hugging Face (ADR-0199)

    private func loadMcp() async {
        if let result = try? await core.request("/api/mcp/servers", timeout: 20) {
            mcpServers = result["items"] as? [[String: Any]] ?? []
        }
    }

    private func mcpCall(_ path: String, body: [String: Any], then: (() async -> Void)? = nil) {
        error = nil
        info = nil
        Task {
            do {
                let result = try await core.request(path, method: "POST", body: body, timeout: 60)
                info = result["message"] as? String ?? "Fatto."
                await loadMcp()
                if let then { await then() }
            } catch { self.error = error.localizedDescription }
        }
    }

    private func readMcpTools(_ name: String) async {
        do {
            let result = try await core.request("/api/mcp/servers/test", method: "POST", body: ["name": name], timeout: 45)
            mcpTools[name] = result["tools"] as? [[String: Any]] ?? []
            info = "Strumenti letti da «\(name)»: \(mcpTools[name]?.count ?? 0)."
        } catch { self.error = error.localizedDescription }
    }

    private func setMcpAuto(_ server: [String: Any], tool: String, on: Bool) {
        guard let name = server["name"] as? String else { return }
        var auto = Set(server["auto"] as? [String] ?? [])
        if on { auto.insert(tool) } else { auto.remove(tool) }
        mcpCall("/api/mcp/servers/policy", body: ["name": name, "auto": Array(auto).sorted()])
    }

    /// Server MCP autorizzati (Hugging Face con un gesto): l'elenco degli strumenti si legge dal server; quelli
    /// dichiarati di sola lettura possono girare senza conferma, tutti gli altri chiedono sempre la tua.
    private var mcpCard: some View {
        let hasHF = mcpServers.contains { ($0["preset"] as? String) == "huggingface" }
        return card("Server MCP e Hugging Face", icon: "puzzlepiece.extension") {
            Text("JARVIS può usare gli strumenti di un server MCP (per esempio cercare modelli e dataset su Hugging Face). Gli strumenti di sola lettura possono girare senza conferma se li approvi qui; tutti gli altri chiedono sempre la tua conferma. Le ricerche partono verso il server: niente dati personali.")
                .font(.callout).foregroundStyle(.secondary)
            if !hasHF {
                Divider()
                Text("Collega Hugging Face").font(.subheadline.weight(.semibold))
                SecureField("Token di sola lettura (facoltativo, da hf.co/settings/mcp)", text: $mcpToken)
                    .textFieldStyle(.roundedBorder)
                Button("Collega Hugging Face", systemImage: "link") {
                    var body: [String: Any] = ["preset": "huggingface", "auto_suggested": true]
                    if !mcpToken.isEmpty { body["token"] = mcpToken }
                    mcpToken = ""
                    mcpCall("/api/mcp/servers", body: body) { await readMcpTools("huggingface") }
                }
            }
            ForEach(Array(mcpServers.enumerated()), id: \.offset) { _, server in
                let name = server["name"] as? String ?? "server"
                Divider()
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(name).font(.headline)
                        Text(server["url"] as? String ?? "").font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer()
                    Label(server["has_token"] as? Bool == true ? "Token salvato" : "Senza token",
                          systemImage: server["has_token"] as? Bool == true ? "key.fill" : "key")
                        .font(.caption).foregroundStyle(.secondary)
                    Button("Leggi strumenti") { Task { await readMcpTools(name) } }
                    Button("Rimuovi", role: .destructive) {
                        mcpTools[name] = nil
                        mcpCall("/api/mcp/servers/remove", body: ["name": name])
                    }
                }
                if let tools = mcpTools[name] {
                    ForEach(Array(tools.enumerated()), id: \.offset) { _, tool in
                        let toolName = tool["name"] as? String ?? "?"
                        let readOnly = tool["read_only"] as? Bool == true
                        let auto = (server["auto"] as? [String] ?? []).contains(toolName)
                        HStack(alignment: .top) {
                            VStack(alignment: .leading, spacing: 1) {
                                Text(toolName).font(.system(.callout, design: .monospaced))
                                Text(String((tool["description"] as? String ?? "").prefix(140)))
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer()
                            if readOnly {
                                Toggle("Senza conferma", isOn: Binding(get: { auto }, set: { setMcpAuto(server, tool: toolName, on: $0) }))
                                    .toggleStyle(.switch).controlSize(.small)
                            } else {
                                Label("Chiede conferma", systemImage: "hand.raised").font(.caption).foregroundStyle(.orange)
                            }
                        }
                    }
                }
            }
        }
        .task { await loadMcp() }
    }

    // MARK: n8n con Docker (ADR-0197)

    private func loadN8n() async {
        if let status = try? await core.request("/api/n8n", timeout: 20) { n8n = status }
    }

    private func n8nCall(_ path: String = "/api/n8n", method: String = "POST", body: [String: Any]) {
        error = nil
        info = nil
        Task {
            do {
                let result = try await core.request(path, method: method, body: body, timeout: 200)
                n8n = result
                info = result["message"] as? String ?? "Fatto."
            } catch { self.error = error.localizedDescription }
            await loadN8n()
        }
    }

    /// n8n gira in un container Docker isolato (senza rete, con soli promemoria e controlli pianificati): JARVIS lo
    /// abilita, lo avvia e lo ferma, e gli passa soltanto modelli interni.
    private var n8nCard: some View {
        let enabled = n8n["enabled"] as? Bool ?? false
        let running = n8n["running"] as? Bool ?? false
        let dockerOK = n8n["docker_connected"] as? Bool ?? true
        let jobs = n8n["items"] as? [[String: Any]] ?? []
        let imageMissing = enabled && dockerOK && n8n["image_present"] as? Bool == false
        let pull = n8n["pull"] as? [String: Any] ?? [:]
        let pulling = pull["status"] as? String == "pulling"
        let estimate = n8n["image_estimate_mb"] as? Int ?? 0
        return card("n8n con Docker", icon: "shippingbox") {
            Text("n8n gira in un container Docker isolato: senza rete, con soli promemoria e controlli pianificati. JARVIS lo avvia e lo ferma; non serve configurare nulla a mano.")
                .font(.callout).foregroundStyle(.secondary)
            if enabled, !dockerOK {
                let hint = (n8n["docker"] as? [String: Any])?["hint"] as? String
                Label(hint ?? (n8n["error"] as? String ?? "Docker non risponde: avvia Docker Desktop."), systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange)
            }
            HStack {
                Label(!enabled ? "Disattivato" : (running ? "In esecuzione" : "Fermo"),
                      systemImage: running ? "checkmark.circle.fill" : "pause.circle")
                    .foregroundStyle(running ? Color.green : .secondary)
                Spacer()
                if !enabled {
                    Button("Abilita n8n") { n8nCall(body: ["enable": true]) }
                } else if running {
                    Button("Ferma") { n8nCall(body: ["running": false]) }
                } else if imageMissing || pulling {
                    Button("Scarica l'immagine e avvia…", systemImage: "arrow.down.circle") { n8nPullConfirm = true }
                        .disabled(pulling)
                } else {
                    Button("Avvia con Docker") { n8nCall(body: ["running": true]) }.disabled(!dockerOK)
                }
                Button("Aggiorna") { Task { await loadN8n() } }
            }
            if imageMissing && !pulling {
                Label("L'immagine di n8n non è sul computer (circa \(estimate) MB): serve scaricarla una volta.", systemImage: "arrow.down.circle")
                    .font(.callout).foregroundStyle(.orange)
            }
            if pulling {
                HStack {
                    ProgressView().controlSize(.small)
                    Text("Download dell'immagine in corso… \(pull["elapsed_s"] as? Int ?? 0) s").font(.callout)
                }
            } else if pull["status"] as? String == "failed" {
                Label(pull["detail"] as? String ?? "Download non riuscito", systemImage: "xmark.octagon").font(.callout).foregroundStyle(.red)
            }
            if enabled && running {
                Divider()
                Text("Nuova routine").font(.subheadline.weight(.semibold))
                TextField("Nome (fino a 80 caratteri)", text: $n8nName).textFieldStyle(.roundedBorder)
                HStack {
                    Picker("Tipo", selection: $n8nKind) {
                        Text("Promemoria").tag("reminder")
                        Text("Controllo").tag("audit")
                    }.pickerStyle(.segmented).frame(maxWidth: 220)
                    Stepper("Ogni \(n8nMinutes) min", value: $n8nMinutes, in: 1...59)
                    Spacer()
                    Button("Crea routine") {
                        let name = n8nName.trimmingCharacters(in: .whitespacesAndNewlines)
                        n8nName = ""
                        n8nCall("/api/n8n/jobs", body: ["name": String(name.prefix(80)), "kind": n8nKind, "minutes": n8nMinutes])
                    }
                    .disabled(n8nName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            ForEach(Array(jobs.enumerated()), id: \.offset) { _, job in
                HStack {
                    Image(systemName: job["kind"] as? String == "audit" ? "checkmark.shield" : "bell")
                    Text(job["name"] as? String ?? "Routine")
                    Text("ogni \(job["minutes"] as? Int ?? 0) min").font(.caption).foregroundStyle(.secondary)
                    Spacer()
                    if let id = job["id"] as? String {
                        Button("Rimuovi", role: .destructive) { n8nCall("/api/n8n/jobs/\(urlSegment(id))", method: "DELETE", body: [:]) }
                    }
                }
            }
        }
        .alert("Scaricare l'immagine di n8n?", isPresented: $n8nPullConfirm) {
            Button("Scarica e avvia") { n8nCall(body: ["pull": true, "running": true]) }
            Button("Annulla", role: .cancel) {}
        } message: {
            Text("Docker scaricherà circa \(estimate) MB da docker.n8n.io (una sola volta) e poi avvierà n8n. Puoi continuare a usare JARVIS durante il download.")
        }
        .task {
            await loadN8n()
            while !Task.isCancelled, section == .workflows {
                if (n8n["pull"] as? [String: Any])?["status"] as? String == "pulling" {
                    try? await Task.sleep(for: .seconds(3))
                    await loadN8n()
                } else {
                    try? await Task.sleep(for: .seconds(8))
                }
            }
        }
    }

    private func addPublicHost() {
        let host = publicHost.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !host.isEmpty else { return }
        publicHost = ""
        mutate("/api/devices/access", method: "PUT", body: ["add": host])
    }

    private var inviteExpiry: Date? {
        guard let value = invite?["expires_at"] as? String else { return nil }
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.date(from: value)
    }

    private func probe(_ origin: String) {
        guard !checkingHTTPS else { return }
        checkingHTTPS = true
        connectionProbe = nil
        Task {
            defer { checkingHTTPS = false }
            do {
                try await core.checkHTTPS(origin)
                connectionProbe = (origin, "Certificato valido · Core raggiungibile", true)
            } catch {
                connectionProbe = (origin, error.localizedDescription, false)
            }
        }
    }

    private func updateInvite(cancel: Bool) {
        guard !changingInvite else { return }
        changingInvite = true
        let previous = invite?["code"] as? String
        var body: [String: Any] = ["scopes": allowDeviceConfirmations ? ["chat", "confirm"] : ["chat"]]
        if let previous { body[cancel ? "code" : "replace"] = previous }
        Task {
            defer { changingInvite = false }
            do {
                let path = cancel ? "/api/devices/pairing/cancel" : "/api/devices/pairing"
                let result = try await core.request(path, method: "POST", body: body, timeout: 10)
                invite = cancel ? nil : result
                error = nil
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    private func copyConnectionText(_ value: String) {
        NSPasteboard.general.clearContents()
        if NSPasteboard.general.setString(value, forType: .string) {
            info = "Copiato negli appunti."
        }
    }

    private var connectorSetup: some View {
        card("Google Workspace", icon: "link.circle") {
            Text("Importa il JSON OAuth Desktop una sola volta: resta salvato (cifrato) per Gmail e Calendario, anche dopo aggiornamenti e revoche. Il consenso si completa nel browser.")
                .foregroundStyle(.secondary)
            let clientSaved = records(payload, key: "templates").contains {
                $0.id == "gmail" && $0.data["client_saved"] as? Bool == true
            }
            if clientSaved {
                Label("Client Google già salvato: non serve reimportare il file.", systemImage: "checkmark.seal")
                    .font(.caption).foregroundStyle(.green)
                HStack {
                    Button("Accedi a Gmail") { Task { await startOAuth("/api/connectors/gmail/oauth/import", body: [:]) } }
                    Button("Accedi a Calendario") { Task { await startOAuth("/api/connectors/google_calendar/oauth/import", body: [:]) } }
                }
            }
            HStack {
                Button(clientSaved ? "Sostituisci il file per Gmail" : "Importa per Gmail") { importGoogle("gmail") }
                Button(clientSaved ? "Sostituisci il file per Calendario" : "Importa per Calendario") { importGoogle("google_calendar") }
            }
            let templates = records(payload, key: "templates")
            if !templates.isEmpty {
                Divider()
                Picker("Catalogo", selection: $selectedTemplate) {
                    Text("Seleziona un servizio").tag("")
                    ForEach(templates) { Text($0.title).tag($0.id) }
                }
                if let entry = templates.first(where: { $0.id == selectedTemplate }) {
                    if records(payload).contains(where: { $0.id == entry.id }) {
                        Label("Già installato", systemImage: "checkmark.circle").foregroundStyle(.secondary)
                    } else {
                        if entry.data["needs_client_id"] as? Bool == true {
                            TextField("ID client OAuth", text: $clientID)
                        }
                        Button("Installa") {
                            mutate("/api/connectors/templates/\(urlSegment(entry.id))", body: ["client_id": clientID])
                        }
                    }
                }
            }
        }
    }

    private func importGoogle(_ id: String) {
        let picker = NSOpenPanel()
        picker.title = "Seleziona le credenziali OAuth Desktop"
        picker.allowedContentTypes = [.json]
        picker.allowsMultipleSelection = false
        picker.begin { result in
            guard result == .OK, let url = picker.url else { return }
            Task { @MainActor in
                do {
                    let file = try FileHandle(forReadingFrom: url)
                    defer { try? file.close() }
                    let data = try file.read(upToCount: 16_385) ?? Data()
                    guard data.count <= 16_384,
                          let body = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                        throw CoreFailure(message: "Seleziona un file JSON OAuth Desktop di massimo 16 KB.")
                    }
                    await startOAuth("/api/connectors/\(id)/oauth/import", body: body)
                } catch { self.error = error.localizedDescription }
            }
        }
    }

    private func connectorSheet(_ item: Record) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(item.title).font(.title2)
            if item["auth"] != "none" {
                SecureField(item["auth"] == "oauth2" ? "Client secret facoltativo" : "Token o chiave API",
                            text: $credential)
            }
            HStack {
                Button("Chiudi") { connectorAction = nil }
                Spacer()
                Button("Revoca", role: .destructive) {
                    mutate("/api/connectors/\(urlSegment(item.id))/revoke", body: [:])
                    connectorAction = nil
                }
                Button("Autorizza") {
                    let path = "/api/connectors/\(urlSegment(item.id))"
                    let value = credential
                    credential = ""
                    connectorAction = nil
                    if item["auth"] == "oauth2" {
                        Task { await startOAuth(path + "/oauth/start", body: ["client_secret": value]) }
                    } else {
                        mutate(path + "/authorize", body: ["credential": value])
                    }
                }.buttonStyle(.borderedProminent)
            }
        }.padding(24).frame(width: 455)
    }

    private func startOAuth(_ path: String, body: [String: Any]) async {
        do {
            let result = try await core.request(path, method: "POST", body: body)
            guard let text = result["authorize_url"] as? String, let url = URL(string: text),
                  url.scheme == "https", url.host != nil, url.user == nil, url.password == nil,
                  NSWorkspace.shared.open(url) else {
                throw CoreFailure(message: "Pagina di consenso OAuth non disponibile.")
            }
            info = "Completa il consenso nel browser, poi aggiorna Connettori."
            await load(.connectors)
        } catch { self.error = error.localizedDescription }
    }

    private func confirmationSheet(_ item: Record) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Verifica l’azione").font(.title2.bold())
            LabeledContent("Strumento", value: item["tool"] ?? "—")
            LabeledContent("Rischio", value: item["risk"] ?? "—")
            if let target = item["target"] { LabeledContent("Destinazione", value: target) }
            if let effect = item["expected_effect"] { Text(effect).textSelection(.enabled) }
            if let expiry = item["expires_at"] { LabeledContent("Scadenza", value: expiry) }
            if let arguments = item.data["arguments"] as? [String: Any] {
                ScrollView { details(arguments) }.frame(maxHeight: 140)
            }
            HStack {
                Button("Annulla") { pending = nil }
                Spacer()
                if let id = item["confirmation_id"] {
                    Button("Rifiuta") { answerConfirmation(id, approve: false) }
                    Button("Approva ed esegui") { answerConfirmation(id, approve: true) }
                        .buttonStyle(.borderedProminent).tint(.orange)
                } else if item["tool"] == "devices.revoke" {
                    Button("Revoca", role: .destructive) {
                        mutate("/api/devices/\(urlSegment(item.id))/revoke", body: [:])
                        pending = nil
                    }
                }
            }
        }.padding(24).frame(width: 470)
    }

    private func answerConfirmation(_ id: String, approve: Bool) {
        pending = nil
        lastReply = nil
        mutate("/api/confirmations/\(urlSegment(id))", body: ["approve": approve])
    }

    private func details(_ item: [String: Any]) -> some View {
        let bytes = try? JSONSerialization.data(withJSONObject: item, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes])
        return Text(bytes.flatMap { String(data: $0, encoding: .utf8) } ?? "—")
            .font(.system(.caption, design: .monospaced)).foregroundStyle(.secondary).textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func load(_ page: Section) async {
        guard section == page, !Task.isCancelled else { return }
        let id = UUID()
        activeLoad = id
        defer {
            if activeLoad == id { activeLoad = nil }
        }
        var path = page.endpoint
        if page == .memory, !memoryQuery.isEmpty {
            var components = URLComponents(string: path)
            components?.queryItems = [URLQueryItem(name: "q", value: memoryQuery)]
            path = components?.string ?? path
        }
        do {
            var result = try await core.request(path)
            if page == .dashboard {
                let status = try? await core.request("/api/cloud", timeout: 3)
                result["providers"] = status?["providers"]
                result["security"] = try? await core.request("/api/security", timeout: 3)
            }
            if page == .plans {
                let templates = try? await core.request("/api/plans/templates", timeout: 5)
                result["templates"] = templates?["items"]
                let suggestions = try? await core.request("/api/improvements", timeout: 5)
                result["improvements"] = suggestions?["items"]
            }
            if page == .chat {
                let chats = try? await core.request("/api/conversations", timeout: 5)
                result["chats"] = chats?["items"]
            }
            if page == .connectors {
                let catalog = try await core.request("/api/connectors/templates")
                result["templates"] = catalog["templates"]
            }
            if page == section, activeLoad == id, !Task.isCancelled {
                payload = result
                if page == .updates {
                    automaticUpdates = result["enabled"] as? Bool ?? false
                }
                if page == .profile { applyProfile(result) }
            }
        } catch {
            if page == section, activeLoad == id, !Task.isCancelled { self.error = error.localizedDescription }
        }
    }

    /// Scarica, verifica (SHA-256) e installa l'aggiornamento senza uscire dall'app (ADR-0109).
    @ViewBuilder private var updateInstaller: some View {
        let download = payload["download"] as? [String: Any] ?? [:]
        let status = download["status"] as? String ?? "idle"
        let available = payload["update_available"] as? Bool ?? false
        let version = download["version"] as? String ?? ""
        if available || status != "idle" {
            card("Aggiornamento automatico", icon: "square.and.arrow.down") {
                if download["supported"] as? Bool != true {
                    Text("Su questo sistema l'aggiornamento resta manuale: scarica la release da GitHub.")
                        .foregroundStyle(.secondary)
                } else if status == "downloading" {
                    let bytes = Double(download["bytes"] as? Int ?? 0)
                    let total = Double(download["total"] as? Int ?? 0)
                    ProgressView(value: total > 0 ? min(bytes / total, 1) : 0) {
                        Text("Scarico \(version): \(Int(bytes / 1_000_000)) / \(Int(total / 1_000_000)) MB")
                    }
                    .task(id: status) {
                        while !Task.isCancelled {
                            try? await Task.sleep(nanoseconds: 1_000_000_000)
                            await load(.updates)
                        }
                    }
                } else if status == "ready" {
                    Text("JARVIS \(version) è scaricato e verificato (SHA-256).")
                    Text("L'app si chiude, installa la nuova versione e si riapre da sola. I dati restano dove sono.")
                        .foregroundStyle(.secondary)
                    Button(applyingUpdate ? "Avvio installazione…" : "Installa e riavvia") {
                        guard !applyingUpdate else { return }
                        applyingUpdate = true
                        error = nil
                        Task {
                            do {
                                _ = try await core.request("/api/updates/install", method: "POST", body: [:])
                                core.stop()
                                NSApplication.shared.terminate(nil)
                            } catch {
                                applyingUpdate = false
                                self.error = error.localizedDescription
                            }
                        }
                    }
                    .disabled(applyingUpdate)
                } else {
                    if status == "failed", let message = download["error"] as? String {
                        Text("Download non riuscito: \(message)").foregroundStyle(.red)
                    }
                    if available {
                        Button("Scarica aggiornamento") { mutate("/api/updates/download", body: [:]) }
                    }
                }
            }
        }
        DisclosureGroup("Registro dell'ultimo aggiornamento") {
            VStack(alignment: .leading, spacing: 8) {
                Button("Leggi il registro") {
                    Task {
                        if let result = try? await core.request("/api/updates/log"), let text = result["log"] as? String {
                            updateLog = text.isEmpty ? "Nessun registro: non è ancora stato tentato un aggiornamento." : text
                        }
                    }
                }
                if !updateLog.isEmpty {
                    Text(updateLog).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding(.top, 6)
        }
        .font(.callout.weight(.medium)).foregroundStyle(.secondary)
    }

    /// Conferma specifica quando il Core non ne manda una: dice cosa è successo, mai un generico «operazione completata».
    private func doneMessage(_ path: String, method: String) -> String {
        let table: [(String, String)] = [
            ("/api/workflows", "Automazione aggiornata."), ("/api/connectors", "Connettore aggiornato."),
            ("/api/devices", "Dispositivi aggiornati."), ("/api/notifications", "Notifiche aggiornate."),
            ("/api/skills", "Competenza salvata."), ("/api/memory", "Memoria aggiornata."),
            ("/api/cloud", "Impostazioni dei modelli salvate."), ("/api/updates", "Aggiornamenti controllati."),
            ("/api/permissions", "Permessi aggiornati."), ("/api/backup", "Copia di sicurezza creata."),
            ("/api/missions", "Missione aggiornata."), ("/api/agents/limits", "Regole dell’agente aggiornate."),
            ("/api/plugins", "Pacchetti aggiornati."), ("/api/users", "Utenti aggiornati."),
        ]
        if let hit = table.first(where: { path.hasPrefix($0.0) }) { return hit.1 }
        return method == "DELETE" ? "Elemento rimosso." : "Modifica salvata."
    }

    private func mutate(_ path: String, method: String = "POST", body: [String: Any]) {
        error = nil
        info = nil
        Task {
            do {
                let result = try await core.request(path, method: method, body: body)
                info = result["message"] as? String ?? result["reply"] as? String ?? doneMessage(path, method: method)
                celebrateTick += 1
                await load(section)
                await core.refreshHealth()
            } catch { self.error = error.localizedDescription }
        }
    }
}
