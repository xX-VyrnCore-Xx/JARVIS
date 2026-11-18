import Foundation

struct DeviceSession: Codable {
    let origin: URL
    let token: String
    let scopes: [String]
}

struct ChatMessage: Decodable, Identifiable {
    let id = UUID()
    let role: String
    let content: String

    private enum CodingKeys: String, CodingKey { case role, content }
}

struct TaskSummary: Decodable, Identifiable {
    let taskId: String
    let objective: String
    let state: String
    let createdAt: String
    var id: String { taskId }
}

struct Notice: Decodable, Identifiable {
    let notificationId: String
    let severity: String
    let title: String
    let body: String?
    let createdAt: String
    let readAt: String?
    var id: String { notificationId }
    var isRead: Bool { readAt != nil }
}

struct Notices: Decodable {
    let unread: Int
    let items: [Notice]
}

struct PendingAction: Decodable, Identifiable {
    let confirmationId: UUID
    let target: String
    let expectedEffect: String
    let expiresAt: String
    var id: UUID { confirmationId }

    var expiration: Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions.insert(.withFractionalSeconds)
        return formatter.date(from: expiresAt) ?? ISO8601DateFormatter().date(from: expiresAt)
    }
}

enum CoreError: LocalizedError {
    case invalidOrigin, invalidResponse, revoked, scopeRequired, invalidPairing
    case rejected(Int, String?)

    var errorDescription: String? {
        switch self {
        case .invalidOrigin: "Inserisci l’indirizzo HTTPS del Core, ad esempio https://jarvis.example."
        case .invalidResponse: "Il Core ha restituito una risposta inattesa."
        case .revoked: "Dispositivo revocato. Genera un nuovo codice per associarlo."
        case .scopeRequired: "Questo dispositivo non può confermare azioni."
        case .invalidPairing: "La risposta di associazione non contiene un token utilizzabile."
        case let .rejected(status, message): message ?? "Il Core ha restituito HTTP \(status)."
        }
    }
}

final class NoRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

struct CoreClient {
    typealias Transport = (URLRequest) async throws -> (Data, URLResponse)
    private static let session: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieStorage = nil
        configuration.urlCache = nil
        return URLSession(configuration: configuration, delegate: NoRedirects(), delegateQueue: nil)
    }()

    private struct PairRequest: Encodable { let code: String; let name: String; let platform = "ios" }
    private struct PairResponse: Decodable {
        struct Device: Decodable { let scopes: [String] }
        let token: String
        let device: Device
    }
    private struct MessageRequest: Encodable { let message: String }
    private struct Decision: Encodable { let approve: Bool }
    private struct Reply: Decodable { let reply: String? }
    private struct List<T: Decodable>: Decodable { let items: [T] }
    private struct Unread: Decodable { let unread: Int }
    private struct Empty: Encodable {}
    private struct ErrorResponse: Decodable {
        struct Detail: Decodable { let message: String }
        let error: Detail
    }

    private let transport: Transport

    init(transport: Transport? = nil) {
        self.transport = transport ?? { try await Self.session.data(for: $0) }
    }

    static func origin(from value: String) throws -> URL {
        guard var parts = URLComponents(string: value.trimmingCharacters(in: .whitespacesAndNewlines)),
              parts.scheme?.lowercased() == "https", let host = parts.host, !host.isEmpty,
              parts.user == nil, parts.password == nil, parts.query == nil, parts.fragment == nil,
              parts.path.isEmpty || parts.path == "/" else { throw CoreError.invalidOrigin }
        parts.path = ""
        guard let url = parts.url else { throw CoreError.invalidOrigin }
        return url
    }

    func pair(origin: URL, code: String, name: String) async throws -> DeviceSession {
        let body = try JSONEncoder().encode(PairRequest(code: code, name: name))
        let result: PairResponse = try await request(origin: origin, path: "/api/devices/pair", body: body)
        guard result.token.utf8.count == 64,
              result.token.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }),
              result.device.scopes.contains("chat") else { throw CoreError.invalidPairing }
        return DeviceSession(origin: origin, token: result.token, scopes: result.device.scopes)
    }

    func history(_ session: DeviceSession) async throws -> [ChatMessage] {
        let result: List<ChatMessage> = try await request(origin: session.origin, path: "/api/conversation?limit=50", token: session.token)
        return result.items
    }

    func pending(_ session: DeviceSession) async throws -> [PendingAction] {
        let result: List<PendingAction> = try await request(origin: session.origin, path: "/api/confirmations", token: session.token)
        return result.items
    }

    /// Attività recenti con il loro stato (permesso `chat`, ADR-0032).
    func tasks(_ session: DeviceSession) async throws -> [TaskSummary] {
        let result: List<TaskSummary> = try await request(origin: session.origin, path: "/api/tasks?limit=30", token: session.token)
        return result.items
    }

    /// Notifiche con il conteggio delle non lette (RF-13).
    func notices(_ session: DeviceSession) async throws -> Notices {
        try await request(origin: session.origin, path: "/api/notifications?limit=30", token: session.token)
    }

    func markAllRead(_ session: DeviceSession) async throws -> Int {
        let body = try JSONEncoder().encode(Empty())
        let result: Unread = try await request(origin: session.origin, path: "/api/notifications/read-all", token: session.token, body: body)
        return result.unread
    }

    func send(_ message: String, session: DeviceSession) async throws -> String? {
        let body = try JSONEncoder().encode(MessageRequest(message: message))
        let result: Reply = try await request(origin: session.origin, path: "/api/chat", token: session.token, body: body)
        return result.reply
    }

    func decide(_ action: PendingAction, approve: Bool, session: DeviceSession) async throws -> String? {
        guard session.scopes.contains("confirm") else { throw CoreError.scopeRequired }
        let body = try JSONEncoder().encode(Decision(approve: approve))
        let result: Reply = try await request(origin: session.origin, path: "/api/confirmations/\(action.id.uuidString.lowercased())", token: session.token, body: body)
        return result.reply
    }

    private func request<T: Decodable>(origin: URL, path: String, token: String? = nil, body: Data? = nil) async throws -> T {
        let base = try Self.origin(from: origin.absoluteString)
        guard let url = URL(string: path, relativeTo: base)?.absoluteURL else { throw CoreError.invalidResponse }
        var request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 150)
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue(token, forHTTPHeaderField: "X-Jarvis-Device")
        if let body {
            request.httpMethod = "POST"
            request.httpBody = body
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        let (data, response) = try await transport(request)
        guard let http = response as? HTTPURLResponse else { throw CoreError.invalidResponse }
        if http.statusCode == 401, token != nil { throw CoreError.revoked }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard (200..<300).contains(http.statusCode) else {
            throw CoreError.rejected(http.statusCode, (try? decoder.decode(ErrorResponse.self, from: data))?.error.message)
        }
        do { return try decoder.decode(T.self, from: data) }
        catch { throw CoreError.invalidResponse }
    }
}
