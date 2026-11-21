import SwiftUI

@main
struct JarvisApp: App {
    var body: some Scene {
        WindowGroup { JarvisView() }
    }
}

struct JarvisView: View {
    @Environment(\.scenePhase) private var scenePhase
    @StateObject private var model = JarvisModel()

    var body: some View {
        NavigationStack {
            Group {
                if !model.restored {
                    ProgressView("Apertura di JARVIS")
                } else if let session = model.session {
                    TabView {
                        chat(session).tabItem { Label("Chat", systemImage: "bubble.left.and.bubble.right") }
                        tasks(session).tabItem { Label("Attività", systemImage: "checklist") }
                        notices(session)
                            .tabItem { Label("Notifiche", systemImage: "bell") }
                            .badge(model.unread)
                    }
                } else {
                    pairing
                }
            }
            .navigationTitle("JARVIS")
            .safeAreaInset(edge: .top) {
                if let error = model.error {
                    HStack {
                        Text(error).font(.footnote)
                        Spacer()
                        Button("Chiudi", systemImage: "xmark") { model.error = nil }.labelStyle(.iconOnly)
                    }
                    .padding().background(.regularMaterial)
                }
            }
        }
        .task { await model.restore() }
        .task(id: scenePhase == .active ? model.session?.token : nil) {
            guard scenePhase == .active, let session = model.session else { return }
            while !Task.isCancelled {
                await model.reload(session, background: true)
                do { try await Task.sleep(for: .seconds(5)) } catch { return }
            }
        }
    }

    private var pairing: some View {
        Form {
            Section {
                TextField("Indirizzo HTTPS", text: $model.host).textContentType(.URL)
                    .keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                TextField("Nome dispositivo", text: $model.name)
                TextField("Codice di associazione", text: $model.code)
                    .textInputAutocapitalization(.characters).autocorrectionDisabled()
                Button("Associa") { Task { await model.pair() } }
                    .disabled(model.busy || model.host.isEmpty || model.code.isEmpty)
            } header: {
                Text("Associa dispositivo")
            } footer: {
                Text("Genera un codice monouso dalla sezione Dispositivi del Core.")
            }
        }
    }

    private static let stateLabel = [
        "succeeded": "riuscita", "failed": "fallita", "waiting_confirmation": "attende conferma",
        "running": "in corso", "cancelled": "annullata", "planned": "pianificata", "pending": "in attesa",
    ]

    /// Attività recenti (ADR-0032).
    private func tasks(_ session: DeviceSession) -> some View {
        List {
            if model.tasks.isEmpty { Text("Nessuna attività.").foregroundStyle(.secondary) }
            ForEach(model.tasks) { task in
                VStack(alignment: .leading, spacing: 4) {
                    Text(task.objective)
                    Text("\(Self.stateLabel[task.state] ?? task.state) · \(task.createdAt.prefix(16).replacingOccurrences(of: "T", with: " "))")
                        .font(.caption).foregroundStyle(task.state == "failed" ? .red : .secondary)
                }
            }
        }
        .refreshable { await model.reload(session) }
    }

    /// Notifiche con le non lette in evidenza (RF-13, ADR-0032).
    private func notices(_ session: DeviceSession) -> some View {
        List {
            if model.notices.isEmpty { Text("Nessuna notifica.").foregroundStyle(.secondary) }
            ForEach(model.notices) { notice in
                VStack(alignment: .leading, spacing: 4) {
                    Text((notice.isRead ? "" : "● ") + notice.title)
                        .font(.headline).foregroundStyle(notice.severity == "error" ? .red : .primary)
                    if let body = notice.body, !body.isEmpty { Text(body).font(.footnote) }
                    Text(notice.createdAt.prefix(16).replacingOccurrences(of: "T", with: " ")).font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .refreshable { await model.reload(session) }
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button("Segna lette") { Task { await model.markAllRead(session) } }.disabled(model.busy || model.unread == 0)
            }
        }
    }

    private func chat(_ session: DeviceSession) -> some View {
        ScrollViewReader { scroll in
            List {
                if model.messages.isEmpty { Text("Scrivi un messaggio per iniziare.").foregroundStyle(.secondary) }
                ForEach(model.messages) { message in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(message.role == "user" ? "Tu" : "JARVIS").font(.caption).foregroundStyle(.secondary)
                        Text(message.content).textSelection(.enabled)
                    }
                    .id(message.id)
                }
                if !model.actions.isEmpty {
                    Section("Conferme in attesa") {
                        ForEach(model.actions) { action in
                            VStack(alignment: .leading, spacing: 4) {
                                Text(action.expectedEffect)
                                Text(action.target).font(.caption).foregroundStyle(.secondary)
                                if let expiration = action.expiration {
                                    Text("Scade alle \(expiration.formatted(date: .omitted, time: .standard))")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                if session.scopes.contains("confirm") {
                                    Button("Valuta richiesta") { model.selection = action }
                                        .disabled(model.busy)
                                }
                            }
                        }
                    }
                }
                if model.busy { ProgressView() }
            }
            .refreshable { await model.reload(session) }
            .onChange(of: model.messages.last?.id) { _, id in
                if let id, model.actions.isEmpty { scroll.scrollTo(id, anchor: .bottom) }
            }
        }
        .safeAreaInset(edge: .bottom) {
            HStack(alignment: .bottom) {
                TextField("Messaggio", text: $model.draft, axis: .vertical).lineLimit(1...4)
                Button("Invia", systemImage: "arrow.up.circle.fill") {
                    Task { await model.send(session) }
                }
                .labelStyle(.iconOnly)
                .disabled(model.busy || model.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .padding().background(.regularMaterial)
        }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Menu("Dispositivo", systemImage: "ellipsis.circle") {
                    Button("Aggiorna") { Task { await model.reload(session) } }
                    Button("Esci su questo dispositivo", role: .destructive) {
                        Task { await model.signOut() }
                    }
                }
                .disabled(model.busy)
            }
        }
        .confirmationDialog("Azione di JARVIS", isPresented: Binding(
            get: { model.selection != nil }, set: { if !$0 { model.selection = nil } }
        ), titleVisibility: .visible, presenting: model.selection) { action in
            Button("Approva") { Task { await model.decide(action, approve: true, session: session) } }
            Button("Rifiuta", role: .destructive) { Task { await model.decide(action, approve: false, session: session) } }
            Button("Chiudi", role: .cancel) {}
        } message: { action in
            Text("\(action.expectedEffect)\n\(action.target)")
        }
    }
}

@MainActor
final class JarvisModel: ObservableObject {
    @Published var session: DeviceSession?
    @Published var host = ""
    @Published var code = ""
    @Published var name = "iPhone"
    @Published var draft = ""
    @Published var messages: [ChatMessage] = []
    @Published var actions: [PendingAction] = []
    @Published var tasks: [TaskSummary] = []
    @Published var notices: [Notice] = []
    @Published var unread = 0
    @Published var selection: PendingAction?
    @Published var error: String?
    @Published var busy = false
    @Published var restored = false
    private let client = CoreClient()

    func restore() async {
        guard !restored else { return }
        await run {
            session = try SessionStore.load()
            restored = true
            if let session {
                host = session.origin.absoluteString
                try await refresh(session)
            }
        }
        restored = true
    }

    func pair() async {
        await run {
            let origin = try CoreClient.origin(from: host)
            let paired = try await client.pair(origin: origin, code: code, name: name)
            code = ""
            try SessionStore.save(paired)
            session = paired
            try await refresh(paired)
        }
    }

    func send(_ session: DeviceSession) async {
        let message = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !message.isEmpty, message.utf8.count <= 8_000 else {
            error = "Usa un messaggio di massimo 8.000 byte."
            return
        }
        await run {
            let reply = try await client.send(message, session: session)
            if draft.trimmingCharacters(in: .whitespacesAndNewlines) == message { draft = "" }
            messages.append(ChatMessage(role: "user", content: message))
            if let reply { messages.append(ChatMessage(role: "assistant", content: reply)) }
            try await refresh(session)
        }
    }

    func decide(_ action: PendingAction, approve: Bool, session: DeviceSession) async {
        await run {
            let reply = try await client.decide(action, approve: approve, session: session)
            actions.removeAll { $0.id == action.id }
            if let reply { messages.append(ChatMessage(role: "assistant", content: reply)) }
            try await refresh(session)
        }
    }

    func reload(_ session: DeviceSession, background: Bool = false) async {
        if background {
            guard !busy, self.session?.token == session.token else { return }
            let revision = sessionRevision
            do { try await refresh(session, revision: revision) }
            catch let error as CoreError {
                if case .revoked = error, self.session?.token == session.token { try? disconnect() }
            } catch { /* La prossima sincronizzazione riprova. */ }
        } else {
            await run { try await refresh(session) }
        }
    }

    func markAllRead(_ session: DeviceSession) async {
        await run {
            unread = try await client.markAllRead(session)
            try await refresh(session)
        }
    }

    func signOut() async {
        await run { try disconnect() }
    }

    private var sessionRevision = 0

    private func refresh(_ session: DeviceSession, revision: Int? = nil) async throws {
        async let history = client.history(session)
        async let pending = client.pending(session)
        async let recent = client.tasks(session)
        async let inbox = client.notices(session)
        let (loadedMessages, loadedActions, loadedTasks, loadedNotices) = try await (history, pending, recent, inbox)
        if let revision, revision != sessionRevision || self.session?.token != session.token { return }
        try Task.checkCancellation()
        messages = loadedMessages
        actions = loadedActions.sorted { $0.expiresAt < $1.expiresAt }
        tasks = loadedTasks
        notices = loadedNotices.items
        unread = loadedNotices.unread
    }

    private func disconnect() throws {
        try SessionStore.remove()
        clearSession()
    }

    private func clearSession() {
        session = nil
        messages = []
        actions = []
        tasks = []
        notices = []
        unread = 0
        selection = nil
        draft = ""
    }

    private func run(_ operation: () async throws -> Void) async {
        guard !busy else { return }
        sessionRevision += 1
        busy = true
        error = nil
        defer { busy = false }
        do { try await operation() }
        catch CoreError.revoked {
            clearSession()
            do {
                try SessionStore.remove()
                error = CoreError.revoked.localizedDescription
            } catch {
                self.error = "\(CoreError.revoked.localizedDescription) \(error.localizedDescription)"
            }
        } catch {
            self.error = error.localizedDescription
        }
    }
}
