import Foundation

@main
struct CoreClientTests {
    static func main() async throws {
        let origin = try CoreClient.origin(from: " https://jarvis.example:8443/ ")
        precondition(origin.absoluteString == "https://jarvis.example:8443")
        for invalid in ["http://jarvis.example", "https://user@jarvis.example", "https://jarvis.example/path",
                        "https://jarvis.example/?x=1", "https://jarvis.example/#token", "/api/chat"] {
            do {
                _ = try CoreClient.origin(from: invalid)
                preconditionFailure("Origine non valida accettata")
            } catch CoreError.invalidOrigin {}
        }

        let token = String(repeating: "a", count: 64)
        let actionId = "8d05e47c-7cb2-4d46-ba41-09b038c6a851"
        let client = CoreClient { request in
            guard let url = request.url else { preconditionFailure("URL mancante") }
            precondition(url.scheme == "https" && url.host == "jarvis.example" && url.port == 8443)
            let path = url.path
            let body = try request.httpBody.map {
                try JSONSerialization.jsonObject(with: $0) as? [String: Any]
            } ?? nil
            let payload: String
            let status: Int
            if path == "/api/devices/pair" {
                precondition(request.value(forHTTPHeaderField: "X-Jarvis-Device") == nil)
                precondition(request.httpMethod == "POST" && body?["platform"] as? String == "ios")
                precondition(body?["name"] as? String == "iPhone")
                if body?["code"] as? String == "invalid" {
                    status = 401
                    payload = #"{"error":{"message":"codice scaduto"}}"#
                } else {
                    status = 200
                    payload = #"{"token":"\#(token)","device":{"scopes":["chat","confirm"]}}"#
                }
            } else {
                let credential = request.value(forHTTPHeaderField: "X-Jarvis-Device")
                if credential == "revoked" {
                    precondition(path == "/api/conversation")
                    status = 401
                    payload = "{}"
                } else {
                    precondition(credential == token)
                    status = 200
                    switch path {
                    case "/api/conversation":
                        precondition(request.httpMethod == "GET" && url.query == "limit=50")
                        payload = #"{"items":[{"role":"assistant","content":"Ciao","sources":[]}]}"#
                    case "/api/chat":
                        precondition(request.httpMethod == "POST" && body?["message"] as? String == "che ore sono")
                        payload = #"{"reply":"Sono le 12."}"#
                    case "/api/confirmations":
                        precondition(request.httpMethod == "GET")
                        payload = #"{"items":[{"confirmation_id":"\#(actionId)","target":"note.txt","expected_effect":"Scrive","expires_at":"2026-11-17T22:00:00.000Z"}]}"#
                    case "/api/tasks":
                        precondition(request.httpMethod == "GET" && url.query == "limit=30")
                        payload = #"{"items":[{"task_id":"t1","objective":"che ore sono","state":"succeeded","created_at":"2026-11-18T10:00:00Z"}]}"#
                    case "/api/notifications":
                        precondition(request.httpMethod == "GET" && url.query == "limit=30")
                        payload = #"{"unread":1,"items":[{"notification_id":"n1","severity":"warning","title":"Conferma richiesta","body":"x","created_at":"2026-11-18T10:00:00Z","read_at":null},{"notification_id":"n2","severity":"info","title":"Fatto","created_at":"2026-11-18T09:00:00Z","read_at":"2026-11-18T09:01:00Z"}]}"#
                    case "/api/notifications/read-all":
                        precondition(request.httpMethod == "POST")
                        payload = #"{"unread":0}"#
                    case "/api/confirmations/\(actionId)":
                        precondition(request.httpMethod == "POST" && body?["approve"] as? Bool == false)
                        payload = #"{"reply":null}"#
                    default:
                        preconditionFailure("Rotta inattesa: \(path)")
                    }
                }
            }
            return (Data(payload.utf8), HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil)!)
        }

        let session = try await client.pair(origin: origin, code: "ABC123", name: "iPhone")
        let history = try await client.history(session)
        precondition(history.first?.content == "Ciao")
        let reply = try await client.send("che ore sono", session: session)
        precondition(reply == "Sono le 12.")
        let actions = try await client.pending(session)
        precondition(actions.count == 1 && actions[0].expiration != nil && actions[0].target == "note.txt")
        let tasks = try await client.tasks(session)
        precondition(tasks.count == 1 && tasks[0].state == "succeeded" && tasks[0].objective == "che ore sono")
        let notices = try await client.notices(session)
        precondition(notices.unread == 1 && notices.items.map(\.isRead) == [false, true])
        let unread = try await client.markAllRead(session)
        precondition(unread == 0)
        let decision = try await client.decide(actions[0], approve: false, session: session)
        precondition(decision == nil)
        do {
            let scope = DeviceSession(origin: origin, token: token, scopes: ["chat"])
            _ = try await client.decide(actions[0], approve: true, session: scope)
            preconditionFailure("Conferma consentita senza permesso")
        } catch CoreError.scopeRequired {}
        do {
            _ = try await client.history(DeviceSession(origin: origin, token: "revoked", scopes: ["chat"]))
            preconditionFailure("Revoca ignorata")
        } catch CoreError.revoked {}
        do {
            _ = try await client.pair(origin: origin, code: "invalid", name: "iPhone")
            preconditionFailure("Codice non valido accettato")
        } catch CoreError.rejected(401, let message) {
            precondition(message == "codice scaduto")
        }

        let redirectClient = CoreClient { request in
            let response = HTTPURLResponse(url: request.url!, statusCode: 307, httpVersion: nil,
                                           headerFields: ["Location": "https://elsewhere.example"])!
            return (Data(), response)
        }
        do {
            _ = try await redirectClient.history(session)
            preconditionFailure("Risposta HTTP 307 accettata")
        } catch CoreError.rejected(307, _) {}
        print("iOS: origine HTTPS, associazione, cronologia, chat, conferme, attività, notifiche, permessi, revoca e rifiuto 307 verificati")
    }
}
