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
        ["objective", "title", "name", "text", "message", "path", "action", "role"]
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
                localizedReason: "Sblocca JARVIS e rendi disponibili i dispositivi associati.") else {
                phase = .locked
                return
            }
            guard authContext === context else { return }
            authContext = nil
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
        authContext?.invalidate()
        authContext = nil
        try? parentPipe?.fileHandleForWriting.close()
        parentPipe = nil
    }

    func refreshHealth() async {
        guard phase == .ready else { return }
        if let status = try? await request("/api/health", timeout: 2) { health = status }
        else { health = [:] }
    }

    func checkHTTPS(_ origin: String) async throws {
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
        let unexpected = CoreFailure(message: "L’indirizzo non restituisce lo stato di connessione di JARVIS.")
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

    func request(_ path: String, method: String = "GET", body: [String: Any]? = nil,
                 timeout: TimeInterval = 190) async throws -> [String: Any] {
        guard process?.isRunning == true, parentPipe != nil else {
            throw CoreFailure(message: "La sessione del Core è chiusa.")
        }
        guard path.hasPrefix("/api/"), let url = URL(string: "http://127.0.0.1:8765" + path) else {
            throw CoreFailure(message: "Indirizzo del Core non valido.")
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
