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

private enum Section: String, CaseIterable, Identifiable {
    case dashboard = "Plancia", chat = "Chat", tasks = "Attività", confirmations = "Conferme"
    case workflows = "Automazioni", memory = "Memoria", workspace = "Workspace", connectors = "Connettori"
    case devices = "Dispositivi", skills = "Competenze", security = "Sicurezza", audit = "Audit"
    case system = "Sistema", models = "Modelli AI", updates = "Aggiornamenti", notifications = "Notifiche"

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
        }
    }
}

private let base = Color(red: 0.025, green: 0.047, blue: 0.083)
private let panel = Color(red: 0.052, green: 0.095, blue: 0.15)
private let iris = Color(red: 0.64, green: 0.68, blue: 1)

@MainActor private struct RootView: View {
    @ObservedObject var core: CoreSession
    @State private var section: Section = .dashboard
    @State private var payload: [String: Any] = [:]
    @State private var message = ""
    @State private var memoryQuery = ""
    @State private var provider = "ollama_cloud"
    @State private var apiKey = ""
    @State private var pending: Record?
    @State private var memoryEdit: Record?
    @State private var memoryText = ""
    @State private var error: String?
    @State private var info: String?
    @State private var sending = false
    @State private var lastReply: [String: Any]?
    @State private var savedFeedback: String?
    @State private var feedbackSaving = false
    @State private var refresh = 0
    @State private var activeLoad: UUID?
    @State private var outgoingMessage: String?
    @FocusState private var composerFocused: Bool
    @State private var selectedTemplate = ""
    @State private var clientID = ""
    @State private var credential = ""
    @State private var connectorAction: Record?
    @State private var backupPassword = ""
    @State private var updateToken = ""
    @State private var automaticUpdates = false

    var body: some View {
        Group {
            if core.phase == .ready {
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
            } else {
                locked
            }
        }
        .background(base)
        .task {
            if NSApplication.shared.isActive { await core.authenticateOnLaunch() }
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
        VStack(alignment: .leading, spacing: 16) {
            Label("JARVIS", systemImage: "smallcircle.filled.circle")
                .font(.title2.bold()).foregroundStyle(.cyan).padding(.horizontal, 10).padding(.top, 8)
            ScrollView {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Section.allCases) { entry in
                        Button { select(entry) } label: {
                            Label(entry.rawValue, systemImage: entry.icon)
                                .foregroundStyle(section == entry ? Color.cyan : Color.primary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(9)
                                .background(section == entry ? Color.cyan.opacity(0.12) : .clear,
                                            in: RoundedRectangle(cornerRadius: 8))
                        }.buttonStyle(.plain)
                            .accessibilityAddTraits(section == entry ? .isSelected : [])
                    }
                }
            }
            Label(core.health.isEmpty ? "Core non raggiungibile" : "Sessione sbloccata",
                  systemImage: "circle.fill")
                .font(.caption).foregroundStyle(core.health.isEmpty ? Color.orange : Color.green)
                .padding(.horizontal, 8)
        }.padding(12).background(panel.opacity(0.56))
    }

    private var compactNavigation: some View {
        HStack {
            Text("JARVIS").font(.headline).foregroundStyle(.cyan)
            Spacer()
            Menu {
                ForEach(Section.allCases) { entry in Button(entry.rawValue) { select(entry) } }
            } label: { Label(section.rawValue, systemImage: section.icon) }
        }.padding(14).background(panel)
    }

    private var locked: some View {
        VStack(spacing: 18) {
            Image(systemName: "lock.shield").font(.system(size: 58, weight: .light)).foregroundStyle(.cyan)
            Text("JARVIS").font(.largeTitle.bold())
            Text("Accedi con Touch ID o con la password del Mac.")
                .foregroundStyle(.secondary)
            if let notice = core.notice { Text(notice).foregroundStyle(.orange).multilineTextAlignment(.center) }
            if core.phase == .starting || core.phase == .authenticating {
                ProgressView(core.phase == .starting ? "Avvio del Core" : "Autenticazione macOS")
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
                Button { select(.notifications) } label: {
                    if let unread = core.health["notifications_unread"] as? Int, unread > 0 {
                        Label("\(unread)", systemImage: "bell.badge")
                    } else {
                        Image(systemName: "bell")
                    }
                }.help("Notifiche").accessibilityLabel("Notifiche")
                Button { refresh += 1 } label: {
                    if activeLoad != nil {
                        ProgressView().controlSize(.small).frame(width: 15, height: 15)
                    } else {
                        Image(systemName: "arrow.clockwise")
                    }
                }.disabled(activeLoad != nil).help("Aggiorna").accessibilityLabel("Aggiorna")
            }.padding(16).background(base)
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
                    }.padding(18).frame(maxWidth: .infinity, alignment: .leading)
                }
                .onChange(of: chatScrollAnchor, initial: true) { _, _ in
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
            dashboardHero
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 155), spacing: 12, alignment: .top)], spacing: 12) {
                dashboardTile("Core", value: coreStatus, icon: "cpu", color: coreIsUp ? .green : .orange,
                              destination: .system)
                dashboardTile("Conferme", value: count("pending_confirmations"), icon: "checkmark.shield",
                              color: (core.health["pending_confirmations"] as? Int ?? 0) > 0 ? .orange : .cyan,
                              destination: .confirmations)
                dashboardTile("Automazioni attive", value: count("automations"), icon: "bolt",
                              color: .cyan, destination: .workflows)
                dashboardTile("Verifica audit", value: auditStatus.0, icon: "lock.shield",
                              color: auditStatus.1, destination: .security)
            }
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .top, spacing: 12) {
                    recentActivity.frame(minWidth: 340)
                    providerStatus.frame(width: 235)
                }
                VStack(alignment: .leading, spacing: 12) {
                    recentActivity
                    providerStatus
                }
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

    private var dashboardHero: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) {
                welcome.frame(minWidth: 255, maxWidth: .infinity, alignment: .leading)
                Avatar(thinking: sending).frame(width: 244)
            }
            VStack(alignment: .leading, spacing: 10) {
                Avatar(thinking: sending).frame(width: 176).frame(maxWidth: .infinity)
                welcome
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            ZStack {
                LinearGradient(colors: [panel, base], startPoint: .topLeading, endPoint: .bottomTrailing)
                RadialGradient(colors: [.cyan.opacity(0.16), .clear], center: .trailing,
                               startRadius: 8, endRadius: 330)
                Canvas { context, size in
                    var lines = Path()
                    for x in stride(from: 0.0, through: size.width, by: 28.0) {
                        lines.move(to: CGPoint(x: x, y: 0))
                        lines.addLine(to: CGPoint(x: x, y: size.height))
                    }
                    for y in stride(from: 0.0, through: size.height, by: 28.0) {
                        lines.move(to: CGPoint(x: 0, y: y))
                        lines.addLine(to: CGPoint(x: size.width, y: y))
                    }
                    context.stroke(lines, with: .color(.cyan.opacity(0.06)), lineWidth: 1)
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 20))
        }
        .overlay { RoundedRectangle(cornerRadius: 20).strokeBorder(.cyan.opacity(0.28)) }
    }

    private var welcome: some View {
        VStack(alignment: .leading, spacing: 14) {
            Label("Sessione proprietario attiva", systemImage: "lock.shield")
                .font(.subheadline.weight(.medium)).foregroundStyle(.cyan)
            Text("JARVIS").font(.system(size: 52, weight: .semibold, design: .rounded))
                .tracking(3).foregroundStyle(.white)
                .accessibilityAddTraits(.isHeader)
            Button { select(.chat) } label: { Label("Apri chat", systemImage: "bubble.left") }
                .buttonStyle(.borderedProminent).controlSize(.large).tint(.cyan)
        }
    }

    private func dashboardTile(_ title: String, value: String, icon: String, color: Color,
                               destination: Section) -> some View {
        Button { select(destination) } label: {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Image(systemName: icon).foregroundStyle(color)
                    Spacer()
                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(.secondary)
                }
                Text(value).font(.title3.weight(.semibold)).monospacedDigit()
                    .lineLimit(1).minimumScaleFactor(0.75)
                Text(title).font(.caption).foregroundStyle(.secondary)
            }
            .padding(14).frame(maxWidth: .infinity, alignment: .leading)
            .background(panel, in: RoundedRectangle(cornerRadius: 12))
            .overlay { RoundedRectangle(cornerRadius: 12).strokeBorder(color.opacity(0.25)) }
            .contentShape(Rectangle())
        }.buttonStyle(.plain)
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
                Avatar(thinking: sending).frame(width: 76)
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
            if history.isEmpty, outgoingMessage == nil {
                Text("Scrivi la tua prima richiesta.").foregroundStyle(.secondary)
            }
            ForEach(history) { item in
                chatBubble(item["content"] ?? item["text"] ?? item.title, user: item["role"] == "user")
            }
            replyFeedback
            if let sources = lastReply?["sources"] as? [[String: Any]], !sources.isEmpty {
                card("Fonti dell’ultima risposta", icon: "link") {
                    ForEach(Array(sources.enumerated()), id: \.offset) { _, source in
                        if let raw = source["url"] as? String, let url = URL(string: raw),
                           ["http", "https"].contains(url.scheme ?? "") {
                            Link(source["title"] as? String ?? raw, destination: url)
                        }
                    }
                }
            }
            if let confirmation = lastReply?["confirmation"] as? [String: Any] {
                Button("Esamina l’azione proposta") { pending = Record(confirmation, index: 0) }
            }
            if let outgoingMessage, lastReply == nil {
                chatBubble(outgoingMessage, user: true)
                ProgressView("JARVIS sta elaborando").controlSize(.small)
            }
        }
    }

    private func chatBubble(_ text: String, user: Bool) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(user ? "Tu" : "JARVIS", systemImage: user ? "person" : "waveform")
                .font(.caption.weight(.semibold))
                .foregroundStyle(user ? iris : .cyan)
            Text(text).textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(14)
        .frame(maxWidth: 740, alignment: .leading)
        .background(user ? iris.opacity(0.10) : panel, in: RoundedRectangle(cornerRadius: 14))
        .overlay { RoundedRectangle(cornerRadius: 14).strokeBorder((user ? iris : .cyan).opacity(0.14)) }
        .padding(user ? .leading : .trailing, 32)
        .frame(maxWidth: .infinity, alignment: user ? .trailing : .leading)
    }

    private var replyTaskID: String? {
        (lastReply?["task"] as? [String: Any])?["task_id"] as? String
    }

    @ViewBuilder private var replyFeedback: some View {
        if let backend = lastReply?["backend"] as? [String: Any],
           backend["feedback_available"] as? Bool == true,
           let id = replyTaskID {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 10) {
                    replyModel(backend)
                    Spacer(minLength: 6)
                    feedbackControls(id)
                }
                VStack(alignment: .leading, spacing: 10) {
                    replyModel(backend)
                    feedbackControls(id)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(panel.opacity(0.72), in: RoundedRectangle(cornerRadius: 10))
        }
    }

    private func replyModel(_ backend: [String: Any]) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(providerName(backend["name"] as? String ?? "")).font(.subheadline.weight(.semibold))
            if let model = backend["model"] as? String {
                Text(model).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func feedbackControls(_ id: String) -> some View {
        HStack(spacing: 8) {
            if savedFeedback != nil {
                Label("Valutazione salvata", systemImage: "checkmark.circle").foregroundStyle(.green)
            } else {
                Button("Utile", systemImage: "hand.thumbsup") { rateReply(id, "helpful") }
            }
            Menu(savedFeedback == nil ? "Migliora" : "Modifica valutazione", systemImage: "slider.horizontal.3") {
                Button("Utile") { rateReply(id, "helpful") }
                Button("Più breve") { rateReply(id, "concise") }
                Button("Più dettagliata") { rateReply(id, "detailed") }
                Button("Più precisa") { rateReply(id, "precise") }
            }
        }
        .font(.caption)
        .disabled(feedbackSaving)
        .help("JARVIS conserva la categoria e il riferimento alla risposta per adattare questo modello.")
    }

    private func rateReply(_ id: String, _ rating: String) {
        guard !feedbackSaving else { return }
        feedbackSaving = true
        Task {
            defer { feedbackSaving = false }
            do {
                _ = try await core.request("/api/cloud/feedback", method: "POST",
                                           body: ["task_id": id, "rating": rating], timeout: 10)
                if replyTaskID == id { savedFeedback = rating }
            } catch {
                if replyTaskID == id { self.error = error.localizedDescription }
            }
        }
    }

    private var composer: some View {
        VStack(spacing: 8) {
            Text("Inferenza cloud").font(.caption).foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack {
                TextField("Messaggio a JARVIS", text: $message)
                    .textFieldStyle(.roundedBorder).focused($composerFocused).onSubmit(send)
                Button("Invia", systemImage: "arrow.up") { send() }
                    .buttonStyle(.borderedProminent)
                    .disabled(sending || message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
    }

    private func send() {
        let text = message.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !sending else { return }
        sending = true
        error = nil
        lastReply = nil
        savedFeedback = nil
        message = ""
        outgoingMessage = text
        Task {
            defer {
                outgoingMessage = nil
                sending = false
            }
            do {
                let result = try await core.request("/api/cloud/chat", method: "POST",
                                                     body: ["message": text, "provider": "auto"])
                lastReply = result
                if section == .chat { await load(.chat) }
                await core.refreshHealth()
            } catch {
                self.error = error.localizedDescription
                if message.isEmpty { message = text }
                if section == .chat { composerFocused = true }
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
        case .connectors: connectorSetup
        case .devices:
            card("Associazione", icon: "qrcode") {
                let urls = payload["pairing_urls"] as? [String] ?? []
                if urls.isEmpty { Text("Configura un indirizzo HTTPS del Core per associare un dispositivo.") }
                else { ForEach(urls, id: \.self) { Text($0).textSelection(.enabled) } }
                Button("Genera codice monouso") {
                    mutate("/api/devices/pairing", body: ["scopes": ["chat", "confirm"]])
                }.disabled(urls.isEmpty)
            }
        case .models:
            card("Provider cloud", icon: "key") {
                Picker("Provider", selection: $provider) {
                    Text("Ollama Cloud").tag("ollama_cloud")
                    Text("NVIDIA NIM").tag("nvidia_nim")
                }
                SecureField("Chiave API", text: $apiKey)
                Button("Salva chiave") {
                    let key = apiKey
                    apiKey = ""
                    mutate("/api/cloud", body: ["provider": provider, "key": key])
                }.disabled(apiKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button("Rimuovi chiave", role: .destructive) {
                    mutate("/api/cloud", body: ["provider": provider, "remove_key": true])
                }
            }
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
                Text(item.data["authorized"] as? Bool == true ? "Autorizzato" : "Da autorizzare")
                Button("Configura") { credential = ""; connectorAction = item }
            }
        case .devices:
            if item.data["revoked_at"] is NSNull {
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
        default: EmptyView()
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
                    if entry.data["needs_client_id"] as? Bool == true { TextField("ID client OAuth", text: $clientID) }
                    Button("Installa") {
                        mutate("/api/connectors/templates/\(urlSegment(entry.id))", body: ["client_id": clientID])
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

    private func mutate(_ path: String, method: String = "POST", body: [String: Any]) {
        error = nil
        info = nil
        Task {
            do {
                let result = try await core.request(path, method: method, body: body)
                if let code = result["code"] as? String { info = "Codice di associazione: \(code)" }
                else { info = result["reply"] as? String ?? "Operazione completata." }
                await load(section)
                await core.refreshHealth()
            } catch { self.error = error.localizedDescription }
        }
    }
}
