import AppKit
import SwiftUI
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

private enum Section: String, Identifiable {
    case dashboard = "Plancia", chat = "Chat", tasks = "Attività", confirmations = "Conferme"
    case workflows = "Automazioni", memory = "Memoria", workspace = "Workspace", connectors = "Connettori"
    case devices = "Dispositivi", skills = "Competenze", security = "Sicurezza", audit = "Audit"
    case system = "Sistema", models = "Modelli AI", updates = "Aggiornamenti", notifications = "Notifiche"
    case missions = "Missioni"

    static let navigation: [(title: String, entries: [Section])] = [
        ("Assistente", [.dashboard, .chat, .tasks, .models, .notifications]),
        ("Organizzazione", [.missions, .workflows, .workspace, .memory, .skills]),
        ("Collegamenti", [.devices, .connectors]),
        ("Amministrazione", [.confirmations, .security, .audit, .system, .updates])
    ]

    var id: Self { self }
    var icon: String {
        switch self {
        case .dashboard: "square.grid.2x2"
        case .chat: "waveform"
        case .tasks: "checklist"
        case .confirmations: "checkmark.shield"
        case .workflows: "bolt"
        case .memory: "cylinder"
        case .workspace: "folder"
        case .connectors: "link"
        case .devices: "laptopcomputer.and.iphone"
        case .skills: "square.stack"
        case .security: "lock.shield"
        case .audit: "list.bullet.rectangle"
        case .system: "cpu"
        case .models: "brain"
        case .updates: "arrow.down.circle"
        case .notifications: "bell"
        case .missions: "scope"
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
    @State private var changingInvite = false

    @State private var section: Section = .dashboard
    @Namespace private var navigationNamespace
    @State private var hoveredEntry: Section?
    @State private var memoryStats: [String: Any] = [:]
    @State private var speakReplies = false
    @State private var systemStats: [String: Any] = [:]
    @State private var missionsPreview: [Record] = []
    @State private var commandText = ""
    @State private var avatarSpeaking = false
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
    @State private var cloudModel = ""
    @State private var provider = "ollama_cloud"
    @State private var apiKey = ""
    @State private var pending: Record?
    @State private var memoryEdit: Record?
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
                    if geometry.size.width >= 860 {
                        HStack(spacing: 0) { sidebar.frame(width: 192); Divider(); content }
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
                .task {
                    if let status = try? await core.request("/api/voice/system") {
                        speakReplies = status["speak_replies"] as? Bool ?? false
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
    }

    private func select(_ value: Section) {
        guard section != value else { return }
        activeLoad = nil
        payload = [:]
        error = nil
        info = nil
        section = value
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 10) {
                ZStack {
                    Circle().fill(LinearGradient(colors: [Color.cyan.opacity(0.35), Color.blue.opacity(0.2)],
                                                 startPoint: .topLeading, endPoint: .bottomTrailing))
                    Circle().strokeBorder(Color.cyan.opacity(0.6), lineWidth: 1.5)
                    Image(systemName: "waveform.path.ecg").font(.system(size: 15, weight: .semibold)).foregroundStyle(.cyan)
                }
                .frame(width: 34, height: 34)
                VStack(alignment: .leading, spacing: 0) {
                    Text("JARVIS").font(.system(size: 17, weight: .bold, design: .rounded))
                    Text("AI Assistant").font(.caption2).foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 6).padding(.top, 6)
            .accessibilityElement(children: .combine)
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    ForEach(Section.navigation.indices, id: \.self) { index in
                        let group = Section.navigation[index]
                        VStack(alignment: .leading, spacing: 3) {
                            Text(group.title.uppercased())
                                .font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                                .padding(.horizontal, 10).padding(.bottom, 2)
                            ForEach(group.entries) { entry in
                                Button { select(entry) } label: {
                                    HStack(spacing: 9) {
                                        Image(systemName: entry.icon).frame(width: 18)
                                        Text(entry.rawValue).font(.callout.weight(section == entry ? .semibold : .regular))
                                        Spacer(minLength: 2)
                                        if let badge = sidebarBadge(entry) {
                                            Text(badge > 99 ? "99+" : "\(badge)")
                                                .font(.caption2.weight(.bold)).monospacedDigit()
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
                                                    .fill(LinearGradient(colors: [Color(red: 0.22, green: 0.42, blue: 0.96).opacity(0.92),
                                                                                  Color(red: 0.32, green: 0.32, blue: 0.86).opacity(0.85)],
                                                                         startPoint: .leading, endPoint: .trailing))
                                                    .shadow(color: Color.blue.opacity(0.35), radius: 8, y: 2)
                                                    .matchedGeometryEffect(id: "navigation-highlight", in: navigationNamespace)
                                            } else if hoveredEntry == entry {
                                                RoundedRectangle(cornerRadius: 10).fill(Color.white.opacity(0.07))
                                            }
                                        }
                                    }
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(PressableStyle())
                                .onHover { inside in
                                    if inside { hoveredEntry = entry } else if hoveredEntry == entry { hoveredEntry = nil }
                                }
                                .accessibilityAddTraits(section == entry ? .isSelected : [])
                            }
                        }
                    }
                }
            }
            sidebarStatus
        }
        .padding(12).background(panel.opacity(0.56))
        .animation(.snappy(duration: 0.28), value: section)
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
            Text(online ? "Modalità: Operativa" : "In attesa del Core").font(.caption2).foregroundStyle(.secondary)
            WaveformView(active: sending, barCount: 18).frame(maxWidth: .infinity).scaleEffect(y: 0.7)
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
            HStack {
                Label(section.rawValue, systemImage: section.icon).font(.title2.weight(.semibold))
                Spacer()
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
                        case .dashboard: dashboard
                        case .chat: chat
                        default: tools
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
            body()
        }
        .padding(15).frame(maxWidth: .infinity, alignment: .leading)
        .background(panel, in: RoundedRectangle(cornerRadius: 14))
        .overlay { RoundedRectangle(cornerRadius: 14).strokeBorder(.cyan.opacity(0.16)) }
    }

    private var dashboard: some View {
        VStack(alignment: .leading, spacing: 18) {
            dashboardHero.staggeredAppear(0)
            commandBar.staggeredAppear(1)
            quickCategories
            attentionCard.staggeredAppear(3)
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
            .staggeredAppear(8)
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

    /// Intestazione della Plancia: saluto, modello attivo, avatar con anelli e carichi del sistema.
    private var dashboardHero: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: 18) {
                VStack(alignment: .leading, spacing: 18) { greetingBlock; aiCard }
                    .frame(maxWidth: .infinity, alignment: .leading)
                heroAvatar(size: 230).frame(width: 290)
                VStack(alignment: .leading, spacing: 14) { systemCard; agentsCard }
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(minWidth: 900)
            VStack(alignment: .leading, spacing: 16) {
                HStack(alignment: .center, spacing: 14) {
                    greetingBlock.frame(maxWidth: .infinity, alignment: .leading)
                    heroAvatar(size: 150).frame(width: 190)
                }
                aiCard
                systemCard
                agentsCard
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
        VStack(alignment: .leading, spacing: 6) {
            Text(timeGreeting).font(.title3).foregroundStyle(.secondary)
            HStack(spacing: 10) {
                Text("Sono").font(.system(size: 38, weight: .bold, design: .rounded)).foregroundStyle(.white)
                Text("JARVIS").font(.system(size: 38, weight: .bold, design: .rounded))
                    .foregroundStyle(LinearGradient(colors: [Color.cyan, Color(red: 0.4, green: 0.55, blue: 1)],
                                                    startPoint: .leading, endPoint: .trailing))
            }
            .accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
            Text(statusHeadline).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
        }
    }

    private func heroAvatar(size: CGFloat) -> some View {
        VStack(spacing: 10) {
            ZStack {
                OrbitRings().frame(width: size, height: size)
                Avatar(thinking: sending && streamingReply.isEmpty, speaking: avatarSpeaking || !streamingReply.isEmpty).frame(width: size * 0.78, height: size * 0.78)
            }
            .frame(width: size, height: size)
            WaveformView(active: sending || avatarSpeaking, barCount: 26).frame(maxWidth: size)
        }
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
        HStack(spacing: 12) {
            Image(systemName: "sparkles").foregroundStyle(.cyan)
            TextField("Parla con JARVIS…", text: $commandText)
                .textFieldStyle(.plain).font(.body).onSubmit(sendCommand)
            Button { sendCommand() } label: { Image(systemName: "paperplane.fill").font(.body.weight(.semibold)) }
                .buttonStyle(PressableStyle()).foregroundStyle(.cyan)
                .disabled(sending || commandText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .accessibilityLabel("Invia")
        }
        .padding(.horizontal, 20).padding(.vertical, 14)
        .background(Capsule().fill(panel))
        .overlay {
            Capsule().strokeBorder(LinearGradient(colors: [Color.cyan.opacity(0.55), Color.indigo.opacity(0.5)],
                                                  startPoint: .leading, endPoint: .trailing), lineWidth: 1.2)
        }
        .frame(maxWidth: 760).frame(maxWidth: .infinity)
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
        case "ollama_cloud": return "Ollama Cloud"
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

    private var chat: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 15) {
                Avatar(thinking: sending && streamingReply.isEmpty, speaking: avatarSpeaking || !streamingReply.isEmpty).frame(width: 76)
                VStack(alignment: .leading, spacing: 5) {
                    Text("JARVIS").font(.title2.weight(.semibold))
                    if sending {
                        Label("Elaborazione in corso", systemImage: "waveform")
                            .foregroundStyle(.cyan)
                    } else {
                        Label("Inferenza cloud", systemImage: "cloud")
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer()
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

    private func send() {
        let text = message.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !sending else { return }
        let includeContext = useCloudContext
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
                if speakReplies, let reply = result["reply"] as? String, !reply.isEmpty {
                    // L'avatar «parla» per la durata stimata della lettura (circa 14 caratteri al secondo).
                    let duration = min(max(Double(reply.count) / 14, 1.5), 45)
                    Task {
                        avatarSpeaking = true
                        _ = try? await core.request("/api/voice/say", method: "POST", body: ["text": reply], timeout: 15)
                        try? await Task.sleep(for: .seconds(duration))
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
            Text("JARVIS parla solo con una voce maschile (Luca).").font(.caption).foregroundStyle(.secondary)
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

    @ViewBuilder private var tools: some View {
        switch section {
        case .memory:
            HStack {
                TextField("Cerca nei ricordi", text: $memoryQuery).onSubmit { refresh += 1 }
                Button("Cerca") { refresh += 1 }
                Button("Reindicizza") { mutate("/api/memory/reindex", body: [:]) }
                Button("Aggiungi ricordo") {
                    select(.chat)
                    message = "ricorda che "
                }
            }
            memoryOverview
        case .missions: missionTools
        case .connectors: connectorSetup
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
            } else if !payload.isEmpty {
                Text("Nessun elemento.").foregroundStyle(.secondary)
            }
        } else {
            ForEach(items) { item in
                card(item.title, icon: section.icon) {
                    if let subtitle = item["state"] ?? item["description"] ?? item["body"] ?? item["kind"] {
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
                Text("Ollama Cloud").tag("ollama_cloud")
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
                Button("Modifica") { memoryText = item["text"] ?? ""; memoryEdit = item }
                Button("Chiedi di dimenticare") { select(.chat); message = "dimentica \(item.id)" }
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
                Button("Briefing adesso") { mutate("/api/proactive/briefing", body: [:]) }
            }
        }
    }

    private var deviceConnection: some View {
        VStack(spacing: 14) {
            card("Connessione HTTPS", icon: "network") {
                let origins = payload["pairing_urls"] as? [String] ?? []
                let origin = origins.contains(remoteOrigin) ? remoteOrigin : origins.first ?? ""
                if origins.isEmpty {
                    Text("Configura un proxy HTTPS privato e autorizza il suo host in remote-hosts.txt.")
                        .foregroundStyle(.secondary)
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
            Text("Importa il JSON OAuth Desktop esistente. Il consenso si completa nel browser.")
                .foregroundStyle(.secondary)
            HStack {
                Button("Importa per Gmail") { importGoogle("gmail") }
                Button("Importa per Calendario") { importGoogle("google_calendar") }
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
                    Button("Installa e riavvia") {
                        Task {
                            do {
                                _ = try await core.request("/api/updates/install", method: "POST", body: [:])
                                NSApplication.shared.terminate(nil)
                            } catch { self.error = error.localizedDescription }
                        }
                    }
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
    }

    private func mutate(_ path: String, method: String = "POST", body: [String: Any]) {
        error = nil
        info = nil
        Task {
            do {
                let result = try await core.request(path, method: method, body: body)
                info = result["reply"] as? String ?? "Operazione completata."
                await load(section)
                await core.refreshHealth()
            } catch { self.error = error.localizedDescription }
        }
    }
}
