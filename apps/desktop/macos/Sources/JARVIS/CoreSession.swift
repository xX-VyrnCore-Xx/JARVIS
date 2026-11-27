import Foundation
import LocalAuthentication
import SwiftUI

struct Record: Identifiable {
    let id: String
    let data: [String: Any]

    init(_ data: [String: Any], index: Int) {
        self.data = data
        if let sequence = data["seq"] as? Int {
            id = "message-\(sequence)"
        } else {
            id = ["id", "confirmation_id", "task_id", "device_id", "workflow_id", "notification_id", "event_id", "connector_id", "name"]
                .compactMap { data[$0] as? String }.first ?? "row-\(index)"
        }
    }

    subscript(_ key: String) -> String? { data[key] as? String }
    var title: String {
        ["objective", "goal", "title", "name", "display_name", "text", "message", "path", "action", "role"]
            .compactMap { self[$0] }.first ?? id
    }
}

struct CoreFailure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

private final class DirectSession: NSObject, URLSessionTaskDelegate {
    func urlSession(
        _ session: URLSession, task: URLSessionTask,
        willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest
    ) async -> URLRequest? { nil }
}

@MainActor final class CoreSession: ObservableObject {
    enum Phase: Equatable { case locked, authenticating, starting, ready, failed }
    @Published private(set) var phase: Phase = .locked
    @Published private(set) var health: [String: Any] = [:]
    @Published var notice: String?
    /// Interfaccia bloccata (ADR-0100): il Core continua a lavorare, ma contenuti e azioni restano nascosti
    /// finché il proprietario non conferma la propria identità.
    @Published private(set) var uiLocked = false
    /// Incrementato a ogni notifica ricevuta in tempo reale (ADR-0098): le viste ricaricano i dati.
    @Published private(set) var liveTick = 0
    private var lastActivity = Date()
    private var lastIdentity = Date.distantPast
    private var observers: [NSObjectProtocol] = []
    private var activityMonitor: Any?
    private var idleTimer: Timer?
    /// Inattività dopo cui l'interfaccia si blocca e validità di una conferma d'identità per le azioni sensibili.
    private static let idleLimit: TimeInterval = 5 * 60
    private static let identityGrace: TimeInterval = 60
    private var token = UUID().uuidString.replacingOccurrences(of: "-", with: "")
        + UUID().uuidString.replacingOccurrences(of: "-", with: "")
    private var authContext: LAContext?
    private var process: Process?
    private var parentPipe: Pipe?
    private var startRequested = false
    private let urlSession: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.connectionProxyDictionary = [:]
        configuration.timeoutIntervalForRequest = 190
        return URLSession(configuration: configuration, delegate: DirectSession(), delegateQueue: nil)
    }()

    init() {
        let workspace = NSWorkspace.shared.notificationCenter
        for name in [NSWorkspace.willSleepNotification, NSWorkspace.screensDidSleepNotification] {
            observers.append(workspace.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                Task { @MainActor in self?.lockInterface(reason: "Yggdrasil si è bloccato perché il Mac è andato in stop.") }
            })
        }
        observers.append(DistributedNotificationCenter.default().addObserver(
            forName: Notification.Name("com.apple.screenIsLocked"), object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in self?.lockInterface(reason: "Yggdrasil si è bloccato insieme allo schermo del Mac.") }
        })
        activityMonitor = NSEvent.addLocalMonitorForEvents(
            matching: [.keyDown, .leftMouseDown, .rightMouseDown, .scrollWheel, .mouseMoved]
        ) { [weak self] event in
            Task { @MainActor in self?.noteActivity() }
            return event
        }
        idleTimer = Timer.scheduledTimer(withTimeInterval: 15, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.checkIdle() }
        }
    }

    func noteActivity() {
        if !uiLocked { lastActivity = Date() }
    }

    private func checkIdle() {
        guard phase == .ready, !uiLocked, Date().timeIntervalSince(lastActivity) > Self.idleLimit else { return }
        lockInterface(reason: "Yggdrasil si è bloccato per inattività.")
    }

    /// Nasconde contenuti e dati in memoria dell'interfaccia. Il Core resta attivo, così missioni, automazioni
    /// e dispositivi associati continuano a funzionare.
    func lockInterface(reason: String) {
        guard phase == .ready, !uiLocked else { return }
        uiLocked = true
        health = [:]
        lastIdentity = .distantPast
        notice = reason
    }

    func unlockInterface() async {
        guard uiLocked else { return }
        if await confirmIdentity("Sblocca Yggdrasil per riprendere la sessione.", force: true) {
            uiLocked = false
            notice = nil
            lastActivity = Date()
            await refreshHealth()
        }
    }

    /// Chiede Touch ID o la password del Mac. Per le azioni sensibili vale per un minuto dopo l'ultima conferma.
    func confirmIdentity(_ reason: String, force: Bool = false) async -> Bool {
        if !force, Date().timeIntervalSince(lastIdentity) < Self.identityGrace { return true }
        let context = LAContext()
        context.localizedCancelTitle = "Annulla"
        var failure: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &failure) else {
            notice = failure?.localizedDescription ?? "Autenticazione macOS non disponibile."
            return false
        }
        do {
            let approved = try await context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason)
            if approved { lastIdentity = Date() }
            return approved
        } catch {
            return false
        }
    }

    /// Notifiche in tempo reale dal Core (ADR-0098). Si ferma con l'interfaccia bloccata o il Core chiuso.
    func watchNotifications() async {
        // Riconnessione con attesa crescente (1, 2, 4… fino a 30 s) e un poco di casualità: dopo un'interruzione il
        // flusso torna in fretta, ma un Core spento non viene martellato. Un flusso riuscito azzera l'attesa.
        var failures = 0
        while !Task.isCancelled, phase == .ready, !uiLocked {
            do {
                guard let url = URL(string: "http://127.0.0.1:8765/api/notifications/stream") else { return }
                var request = URLRequest(url: url, timeoutInterval: 60)
                request.setValue(token, forHTTPHeaderField: "X-Jarvis-Token")
                request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
                let (bytes, response) = try await urlSession.bytes(for: request)
                guard (response as? HTTPURLResponse)?.statusCode == 200 else {
                    throw CoreFailure(message: "Flusso notifiche non disponibile.")
                }
                // `lines` omette le righe vuote: l'evento si riconosce dalla riga «event:».
                failures = 0
                for try await line in bytes.lines where line.hasPrefix("event:") {
                    let name = line.dropFirst(6).trimmingCharacters(in: .whitespaces)
                    if name == "notification" || name == "resync" {
                        liveTick += 1
                        await refreshHealth()
                    }
                    if uiLocked || phase != .ready { break }
                }
            } catch {
                // Si riprova: il polling di salute resta la rete di sicurezza.
                failures += 1
            }
            let wait = min(30.0, pow(2.0, Double(min(failures, 5)))) * Double.random(in: 0.8...1.2)
            try? await Task.sleep(for: .seconds(max(1.0, wait)))
        }
    }

    func authenticateOnLaunch() async {
        guard !startRequested else { return }
        startRequested = true
        await unlock()
    }

    func unlock() async {
        guard phase == .locked || phase == .failed, process == nil else { return }
        phase = .authenticating
        notice = nil
        let context = LAContext()
        authContext = context
        context.localizedCancelTitle = "Esci"
        var reason: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &reason) else {
            notice = reason?.localizedDescription ?? "Autenticazione macOS non disponibile."
            phase = .locked
            authContext = nil
            return
        }
        do {
            guard try await context.evaluatePolicy(.deviceOwnerAuthentication,
                localizedReason: "Sblocca Yggdrasil e rendi disponibili i dispositivi associati.") else {
                phase = .locked
                return
            }
            guard authContext === context else { return }
            authContext = nil
            uiLocked = false
            lastActivity = Date()
            lastIdentity = Date()
            phase = .starting
            try launch()
            for _ in 0..<80 {
                guard phase == .starting, process?.isRunning == true else {
                    throw CoreFailure(message: "Il Core si è arrestato. Consulta core.log nella cartella dati.")
                }
                if let status = try? await request("/api/tools", timeout: 1),
                   status["tools"] is [[String: Any]] || status["items"] is [[String: Any]] {
                    let status = (try? await request("/api/health", timeout: 2)) ?? [:]
                    guard phase == .starting, process?.isRunning == true else {
                        throw CoreFailure(message: "Il Core si è arrestato durante l’avvio.")
                    }
                    health = status
                    phase = .ready
                    return
                }
                try await Task.sleep(for: .milliseconds(250))
            }
            throw CoreFailure(message: "Il Core non risponde sulla porta 8765. Controlla eventuali processi già avviati.")
        } catch {
            authContext = nil
            stop()
            notice = error.localizedDescription
            phase = error is LAError ? .locked : .failed
        }
    }

    private func launch() throws {
        let env = ProcessInfo.processInfo.environment
        let resources = Bundle.main.resourceURL ?? Bundle.main.bundleURL
        let root = env["JARVIS_HOME"].map { URL(fileURLWithPath: $0) } ?? resources
        let executable = env["JARVIS_CORE_BIN"].map { URL(fileURLWithPath: $0) }
            ?? resources.appendingPathComponent("jarvis-core")
        let python = env["JARVIS_PYTHON"] ?? resources.appendingPathComponent("python/bin/python3").path
        let standardData = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                                       appropriateFor: nil, create: true).appendingPathComponent("JARVIS")
        let data = env["JARVIS_DATA_DIR"].map { URL(fileURLWithPath: $0) } ?? standardData
        try FileManager.default.createDirectory(at: data, withIntermediateDirectories: true,
                                                attributes: [.posixPermissions: 0o700])
        let marker = data.appendingPathComponent(".macos-owner-session")
        if !FileManager.default.fileExists(atPath: marker.path) {
            try Data().write(to: marker, options: .atomic)
        }
        let log = data.appendingPathComponent("core.log")
        if !FileManager.default.fileExists(atPath: log.path) {
            _ = FileManager.default.createFile(atPath: log.path, contents: nil,
                                           attributes: [.posixPermissions: 0o600])
        }
        let output = try FileHandle(forWritingTo: log)
        try output.truncate(atOffset: 0)
        let pipe = Pipe()
        let child = Process()
        child.executableURL = executable
        child.currentDirectoryURL = root
        child.environment = env.merging([
            "JARVIS_HOST": "127.0.0.1", "JARVIS_PORT": "8765",
            "JARVIS_HOME": root.path, "JARVIS_PYTHON": python,
            "JARVIS_DATA_DIR": data.path, "JARVIS_TOKEN": token,
            "JARVIS_EXIT_WITH_PARENT": "stdin", "JARVIS_MACOS_SESSION": "1"
        ]) { _, new in new }
        child.standardInput = pipe
        child.standardOutput = output
        child.standardError = output
        child.terminationHandler = { [weak self] ended in
            Task { @MainActor in
                guard let self, self.process === ended else { return }
                self.process = nil
                self.parentPipe = nil
                self.phase = .failed
                self.notice = "Il Core si è arrestato. I dispositivi non sono più connessi."
            }
        }
        try child.run()
        process = child
        parentPipe = pipe
        try pipe.fileHandleForWriting.write(contentsOf: Data((token + "\n").utf8))
    }

    func stop() {
        uiLocked = false
        authContext?.invalidate()
        authContext = nil
        try? parentPipe?.fileHandleForWriting.close()
        parentPipe = nil
    }

    func refreshHealth() async {
        guard phase == .ready, !uiLocked else { return }
        if let status = try? await request("/api/health", timeout: 2) { health = status }
        else { health = [:] }
    }

    /// Verifica l'indirizzo HTTPS (ADR-0192). Un nome Tailscale appena pubblicato può non risolversi o non avere ancora il
    /// certificato per qualche secondo: in quel caso si riprova fino a tre volte prima di arrendersi, e l'errore
    /// finale dice che cosa controllare invece di «A server with the specified hostname could not be found».
    func checkHTTPS(_ origin: String) async throws {
        let host = URLComponents(string: origin)?.host ?? origin
        var attempt = 0
        while true {
            do {
                try await probeHTTPS(origin)
                return
            } catch let failure as CoreFailure {
                throw failure
            } catch {
                attempt += 1
                let retryable = (error as? URLError).map { Self.isTransient($0.code) } ?? false
                if retryable, host.hasSuffix(".ts.net"), attempt < 3 {
                    try await Task.sleep(nanoseconds: 3_000_000_000)
                    continue
                }
                throw CoreFailure(message: Self.explainNetwork(error, host: host))
            }
        }
    }

    private static func isTransient(_ code: URLError.Code) -> Bool {
        switch code {
        case .cannotFindHost, .dnsLookupFailed, .timedOut, .secureConnectionFailed,
             .serverCertificateUntrusted, .serverCertificateHasUnknownRoot, .serverCertificateNotYetValid:
            return true
        default:
            return false
        }
    }

    /// Spiega in italiano gli errori di rete più comuni, con l'azione da fare.
    static func explainNetwork(_ error: Error, host: String) -> String {
        guard let failure = error as? URLError else { return error.localizedDescription }
        let tailnet = host.hasSuffix(".ts.net")
        switch failure.code {
        case .cannotFindHost, .dnsLookupFailed:
            return tailnet
                ? "Il nome \(host) non si trova. Apri Tailscale su questo dispositivo: deve essere connesso con lo stesso account e con «Use Tailscale DNS settings» attivo. Su login.tailscale.com/admin/dns controlla anche che MagicDNS e HTTPS siano abilitati."
                : "Il nome \(host) non si trova: controlla l'indirizzo e la connessione di rete."
        case .notConnectedToInternet, .networkConnectionLost:
            return "Nessuna connessione di rete: controlla la rete o la connessione a Tailscale."
        case .timedOut:
            return tailnet
                ? "Nessuna risposta da \(host): il computer con Yggdrasil potrebbe essere spento o Tailscale scollegato."
                : "Nessuna risposta da \(host) entro il tempo previsto."
        case .cannotConnectToHost:
            return "\(host) non accetta connessioni: controlla che il Core sia avviato e pubblicato con «Collega con Tailscale»."
        case .secureConnectionFailed, .serverCertificateUntrusted, .serverCertificateHasUnknownRoot, .serverCertificateNotYetValid:
            return tailnet
                ? "Il certificato HTTPS di \(host) non è ancora pronto: Tailscale lo crea al primo accesso, riprova fra qualche secondo. Se persiste, abilita «HTTPS Certificates» su login.tailscale.com/admin/dns."
                : "Il certificato HTTPS di \(host) non è valido."
        default:
            return failure.localizedDescription
        }
    }

    private func probeHTTPS(_ origin: String) async throws {
        guard var endpoint = URLComponents(string: origin), endpoint.scheme == "https",
              endpoint.host != nil, endpoint.user == nil, endpoint.password == nil,
              endpoint.query == nil, endpoint.fragment == nil, endpoint.path.isEmpty else {
            throw CoreFailure(message: "Origine HTTPS non valida.")
        }
        endpoint.path = "/api/connectivity"
        guard let url = endpoint.url else { throw CoreFailure(message: "Origine HTTPS non valida.") }
        var request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 8)
        request.httpShouldHandleCookies = false
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForResource = 10
        let probeSession = URLSession(configuration: configuration, delegate: DirectSession(), delegateQueue: nil)
        defer { probeSession.invalidateAndCancel() }
        let (bytes, response) = try await probeSession.bytes(for: request)
        let unexpected = CoreFailure(message: "L’indirizzo non restituisce lo stato di connessione di Yggdrasil.")
        guard (response as? HTTPURLResponse)?.statusCode == 200 else { throw unexpected }
        var data = Data()
        for try await byte in bytes {
            guard data.count < 4096 else { throw unexpected }
            data.append(byte)
        }
        guard let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              object["service"] as? String == "jarvis-core", object["protocol"] as? Int == 1 else {
            throw unexpected
        }
    }

    /// Operazioni che cambiano chi può accedere a JARVIS o dove vanno i dati: dispositivi, chiavi cloud,
    /// connettori, backup, passkey, aggiornamenti e canali di notifica. Chiedono Touch ID o la password del Mac.
    private static func isSensitive(_ path: String, method: String) -> Bool {
        guard method != "GET" else { return false }
        let route = path.split(separator: "?").first.map(String.init) ?? path
        if route == "/api/cloud" { return true }
        return ["/api/devices", "/api/connectors", "/api/backup", "/api/passkeys", "/api/updates", "/api/notify/channels"]
            .contains { route == $0 || route.hasPrefix($0 + "/") }
    }

    /// Invia un audio WAV (ADR-0134): serve all'ascolto «Hey Jarvis» e alla trascrizione; restituisce il JSON del Core.
    func postAudio(_ path: String, wav: Data, timeout: TimeInterval = 60) async throws -> [String: Any] {
        guard process?.isRunning == true, parentPipe != nil else {
            throw CoreFailure(message: "La sessione del Core è chiusa.")
        }
        guard path.hasPrefix("/api/voice/"), let url = URL(string: "http://127.0.0.1:8765" + path) else {
            throw CoreFailure(message: "Indirizzo del Core non valido.")
        }
        if uiLocked {
            throw CoreFailure(message: "Yggdrasil è bloccato: sblocca l’interfaccia per continuare.")
        }
        var request = URLRequest(url: url, timeoutInterval: timeout)
        request.httpMethod = "POST"
        request.setValue(token, forHTTPHeaderField: "X-Jarvis-Token")
        request.setValue("audio/wav", forHTTPHeaderField: "Content-Type")
        request.httpBody = wav
        let (data, response) = try await urlSession.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw CoreFailure(message: "Risposta del Core assente.")
        }
        let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        guard (200..<300).contains(http.statusCode) else {
            let error = object?["error"] as? [String: Any]
            throw CoreFailure(message: error?["message"] as? String ?? "Il Core ha risposto HTTP \(http.statusCode).")
        }
        return object ?? [:]
    }

    /// Chat in streaming (ADR-0131): righe NDJSON `delta` (testo), `done` (esito) o `error`. `onDelta` riceve ogni
    /// frammento di testo; il risultato è l'esito finale, lo stesso oggetto di `request`.
    func stream(_ path: String, body: [String: Any], timeout: TimeInterval = 190,
                onDelta: @MainActor (String) -> Void) async throws -> [String: Any] {
        guard process?.isRunning == true, parentPipe != nil else {
            throw CoreFailure(message: "La sessione del Core è chiusa.")
        }
        guard path.hasPrefix("/api/"), let url = URL(string: "http://127.0.0.1:8765" + path) else {
            throw CoreFailure(message: "Indirizzo del Core non valido.")
        }
        if uiLocked {
            throw CoreFailure(message: "Yggdrasil è bloccato: sblocca l’interfaccia per continuare.")
        }
        var request = URLRequest(url: url, timeoutInterval: timeout)
        request.httpMethod = "POST"
        request.setValue(token, forHTTPHeaderField: "X-Jarvis-Token")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
        let (bytes, response) = try await urlSession.bytes(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw CoreFailure(message: "Risposta del Core assente.")
        }
        guard (200..<300).contains(http.statusCode) else {
            var data = Data()
            for try await byte in bytes { data.append(byte) }
            let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
            let error = object?["error"] as? [String: Any]
            throw CoreFailure(message: error?["message"] as? String ?? "Il Core ha risposto HTTP \(http.statusCode).")
        }
        for try await line in bytes.lines {
            guard let data = line.data(using: .utf8),
                  let event = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { continue }
            switch event["type"] as? String {
            case "delta":
                onDelta(event["text"] as? String ?? "")
            case "done":
                return event["result"] as? [String: Any] ?? [:]
            case "error":
                let error = event["error"] as? [String: Any]
                throw CoreFailure(message: error?["message"] as? String ?? "Errore del Core.")
            default:
                break
            }
        }
        throw CoreFailure(message: "Il flusso si è interrotto prima della risposta finale.")
    }

    func request(_ path: String, method: String = "GET", body: [String: Any]? = nil,
                 timeout: TimeInterval = 190) async throws -> [String: Any] {
        guard process?.isRunning == true, parentPipe != nil else {
            throw CoreFailure(message: "La sessione del Core è chiusa.")
        }
        guard path.hasPrefix("/api/"), let url = URL(string: "http://127.0.0.1:8765" + path) else {
            throw CoreFailure(message: "Indirizzo del Core non valido.")
        }
        if uiLocked {
            throw CoreFailure(message: "Yggdrasil è bloccato: sblocca l’interfaccia per continuare.")
        }
        if Self.isSensitive(path, method: method),
           !(await confirmIdentity("Conferma la tua identità per questa operazione.")) {
            throw CoreFailure(message: "Operazione annullata: identità non confermata.")
        }
        var request = URLRequest(url: url, timeoutInterval: timeout)
        request.httpMethod = method
        request.setValue(token, forHTTPHeaderField: "X-Jarvis-Token")
        if let body {
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        let (data, response) = try await urlSession.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw CoreFailure(message: "Risposta del Core assente.")
        }
        guard (200..<300).contains(http.statusCode) else {
            let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
            let error = object?["error"] as? [String: Any]
            throw CoreFailure(message: error?["message"] as? String ?? "Il Core ha risposto HTTP \(http.statusCode).")
        }
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw CoreFailure(message: "Risposta del Core non valida.")
        }
        return object
    }
}

func records(_ object: [String: Any], key: String = "items") -> [Record] {
    (object[key] as? [[String: Any]] ?? []).enumerated().map { Record($0.element, index: $0.offset) }
}

func urlSegment(_ value: String) -> String {
    value.addingPercentEncoding(withAllowedCharacters: CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-_"))) ?? ""
}
