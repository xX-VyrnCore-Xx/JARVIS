import AppKit
import SwiftUI
import UniformTypeIdentifiers

@MainActor final class AppLifecycle: NSObject, NSApplicationDelegate {
    let core = CoreSession()
    func applicationDidBecomeActive(_ notification: Notification) {
        Task { await core.authenticateOnLaunch() }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) { core.stop() }
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
    case dashboard = "Plancia", chat = "Chat e voce", tasks = "Attività", confirmations = "Conferme"
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

@MainActor private struct RootView: View {
    @ObservedObject var core: CoreSession
    @StateObject private var voice = Voice()
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
    @State private var microphoneBusy = false
    @State private var lastReply: [String: Any]?
    @State private var refresh = 0
    @State private var selectedTemplate = ""
    @State private var clientID = ""
    @State private var credential = ""
    @State private var connectorAction: Record?
    @State private var stt = "small"
    @State private var tts = "it_IT-paola-medium"
    @State private var backupPassword = ""
    @State private var updateToken = ""
    @State private var automaticUpdates = false
    @AppStorage("speakReplies") private var speakReplies = true
    @AppStorage("cloudChat") private var cloudChat = false

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
                        if section == .system, (payload["setup"] as? [String: Any])?["running"] as? Bool == true {
                            await load(.system)
                        }
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
        .onChange(of: core.phase) { _, phase in
            if phase != .ready { voice.cancelRecording(); voice.stopPlayback() }
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
                    Label("\(core.health["notifications_unread"] as? Int ?? 0)", systemImage: "bell")
                }.help("Notifiche")
                Button { refresh += 1 } label: { Image(systemName: "arrow.clockwise") }.help("Aggiorna")
            }.padding(16).background(base)
            Divider()
            if let error { banner(error, color: .orange) }
            if let info { banner(info, color: .cyan) }
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    switch section {
                    case .dashboard: dashboard
                    case .chat: chat
                    default: tools
                    }
                }.padding(18).frame(maxWidth: .infinity, alignment: .leading)
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
        VStack(alignment: .leading, spacing: 15) {
            ViewThatFits(in: .horizontal) {
                HStack { welcome.frame(minWidth: 220); Avatar(voice: voice, thinking: sending).frame(width: 245) }
                VStack { Avatar(voice: voice, thinking: sending).frame(width: 170); welcome }
            }
            .padding(18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(LinearGradient(colors: [panel, base], startPoint: .topTrailing, endPoint: .bottomLeading),
                        in: RoundedRectangle(cornerRadius: 16))
            .overlay { RoundedRectangle(cornerRadius: 16).strokeBorder(.cyan.opacity(0.2)) }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 178), alignment: .top)], alignment: .leading) {
                card("Core", icon: "cpu") {
                    let status = core.health["core"] as? [String: Any]
                    Text(status?["status"] as? String == "up" ? "Operativo" : "In attesa di stato")
                    if let version = status?["version"] as? String { Text("Versione \(version)").foregroundStyle(.secondary) }
                }
                card("Conferme", icon: "checkmark.shield") {
                    Text(core.health["pending_confirmations"].map { "\($0)" } ?? "—").font(.title2.monospacedDigit())
                    Button("Esamina richieste") { select(.confirmations) }
                }
                card("Automazioni", icon: "bolt") {
                    Text(core.health["automations"].map { "\($0)" } ?? "—").font(.title2.monospacedDigit())
                    Button("Apri automazioni") { select(.workflows) }
                }
            }
            card("Attività recenti", icon: "clock") {
                let latest = records(payload).prefix(5)
                if latest.isEmpty { Text("Nessuna attività recente.").foregroundStyle(.secondary) }
                ForEach(latest) { item in
                    LabeledContent { Text(item["state"] ?? "—").foregroundStyle(.secondary) } label: {
                        Text(item.title).lineLimit(2)
                    }
                }
                Button("Tutte le attività") { select(.tasks) }
            }
        }
    }

    private var welcome: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Sono JARVIS").font(.system(size: 40, weight: .bold)).foregroundStyle(.cyan)
            Text("Gestisci attività, dispositivi e automazioni dal tuo Mac.").foregroundStyle(.secondary)
            Button { select(.chat) } label: { Label("Parla con JARVIS", systemImage: "waveform") }
                .buttonStyle(.borderedProminent).controlSize(.large)
        }
    }

    private var chat: some View {
        VStack(alignment: .leading, spacing: 14) {
            Avatar(voice: voice, thinking: sending).frame(width: 150).frame(maxWidth: .infinity)
            let history = records(payload)
            if history.isEmpty { Text("Scrivi o detta la tua prima richiesta.").foregroundStyle(.secondary) }
            ForEach(history) { item in
                card(item["role"] == "user" ? "Tu" : "JARVIS",
                     icon: item["role"] == "user" ? "person" : "waveform") {
                    Text(item["content"] ?? item["text"] ?? item.title).textSelection(.enabled)
                }
            }
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
            if sending { ProgressView("JARVIS sta elaborando") }
        }
    }

    private var composer: some View {
        VStack(spacing: 8) {
            HStack {
                Toggle("Voce", isOn: $speakReplies).toggleStyle(.switch).controlSize(.small)
                Toggle("Cloud", isOn: $cloudChat).toggleStyle(.switch).controlSize(.small)
                    .help("Invia il messaggio al provider AI configurato")
                Spacer()
                if voice.playing {
                    Button("Interrompi voce", systemImage: "speaker.slash") { voice.stopPlayback() }
                }
            }
            HStack {
                Button {
                    microphoneBusy = true
                    Task {
                        defer { microphoneBusy = false }
                        do {
                            if voice.recording { message = try await voice.transcribe(through: core) }
                            else { try await voice.startRecording() }
                        } catch { self.error = error.localizedDescription }
                    }
                } label: {
                    Image(systemName: voice.recording ? "stop.circle.fill" : "mic")
                }
                .disabled(microphoneBusy)
                .help(voice.recording ? "Ferma e trascrivi" : "Registra")
                TextField("Messaggio a JARVIS", text: $message)
                    .textFieldStyle(.roundedBorder).onSubmit(send)
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
        message = ""
        voice.stopPlayback()
        Task {
            do {
                let result = try await core.request(cloudChat ? "/api/cloud/chat" : "/api/chat", method: "POST",
                                                     body: ["message": text, "provider": "auto"])
                lastReply = result
                await load(.chat)
                sending = false
                if speakReplies, let reply = result["reply"] as? String, !reply.isEmpty {
                    await voice.speak(reply, through: core)
                }
            } catch {
                sending = false
                self.error = error.localizedDescription
                if message.isEmpty { message = text }
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
            voiceSetup
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

    private var voiceSetup: some View {
        card("Voce locale", icon: "mic") {
            let setup = payload["setup"] as? [String: Any] ?? [:]
            let choices = setup["choices"] as? [String: Any] ?? [:]
            let sttModels = choices["stt_models"] as? [String] ?? []
            let ttsVoices = choices["tts_voices"] as? [String] ?? []
            let running = setup["running"] as? Bool == true
            if !sttModels.isEmpty, !ttsVoices.isEmpty {
                Picker("Riconoscimento", selection: $stt) {
                    ForEach(sttModels, id: \.self) { Text($0).tag($0) }
                }.disabled(running)
                Picker("Voce", selection: $tts) {
                    ForEach(ttsVoices, id: \.self) { Text($0).tag($0) }
                }.disabled(running)
                if running {
                    ProgressView("Preparazione del motore vocale")
                } else {
                    Button("Configura voce locale") {
                        mutate("/api/setup/start", body: ["stt_model": stt, "tts_voice": tts])
                    }
                }
            }
            let current = records(setup, key: "steps")
            let saved = setup["saved"] as? [String: Any] ?? [:]
            ForEach(current.isEmpty ? records(saved, key: "steps") : current) { item in
                LabeledContent(item["label"] ?? item.id, value: item["status"] ?? "—")
            }
            Button("Aggiorna configurazione") { refresh += 1 }
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
        var path = page.endpoint
        if page == .memory, !memoryQuery.isEmpty {
            var components = URLComponents(string: path)
            components?.queryItems = [URLQueryItem(name: "q", value: memoryQuery)]
            path = components?.string ?? path
        }
        do {
            var result = try await core.request(path)
            if page == .connectors {
                let catalog = try await core.request("/api/connectors/templates")
                result["templates"] = catalog["templates"]
            }
            if page == .system {
                result["setup"] = try await core.request("/api/setup")
            }
            if page == section, !Task.isCancelled {
                payload = result
                if page == .updates {
                    automaticUpdates = result["enabled"] as? Bool ?? false
                }
            }
        } catch {
            if page == section, !Task.isCancelled { self.error = error.localizedDescription }
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
