package local.jarvis.client

import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL

/** Sessione di un dispositivo associato (RF-11): il token resta solo sul telefono e nelle richieste. */
data class DeviceSession(val origin: String, val token: String, val scopes: List<String>) {
    val canConfirm: Boolean get() = "confirm" in scopes
}

data class ChatMessage(val role: String, val content: String)

data class TaskSummary(val id: String, val objective: String, val state: String, val createdAt: String)

data class Notice(
    val id: String,
    val severity: String,
    val category: String,
    val title: String,
    val body: String,
    val createdAt: String,
    val read: Boolean,
    val link: NotificationLink? = null,
)

data class NotificationLink(val view: String, val ref: String?)

private val notificationViews = setOf(
    "chat", "tasks", "automations", "audit", "memory",
    "workspace", "connectors", "system", "devices", "dashboard",
)

data class Notices(val unread: Int, val items: List<Notice>)

data class PendingAction(val confirmationId: String, val target: String, val expectedEffect: String, val expiresAt: String)

sealed class CoreException(message: String) : Exception(message) {
    class InvalidOrigin : CoreException("Inserisci l'indirizzo HTTPS del Core, ad esempio https://jarvis.example.")
    class InvalidResponse : CoreException("Il Core ha restituito una risposta inattesa.")
    class Revoked : CoreException("Dispositivo revocato. Genera un nuovo codice per associarlo.")
    class ScopeRequired : CoreException("Questo dispositivo non può confermare azioni.")
    class InvalidPairing : CoreException("La risposta di associazione non contiene un token utilizzabile.")
    class InvalidPushToken : CoreException("Token di notifica non valido.")
    class Rejected(val status: Int, detail: String?) : CoreException(detail ?: "Il Core ha restituito HTTP $status.")
}

/** Risposta HTTP minima, separata dal trasporto per poterla simulare nei test. */
data class HttpReply(val status: Int, val body: String)

fun interface Transport {
    fun send(method: String, url: String, headers: Map<String, String>, body: String?): HttpReply
}

/** Trasporto reale: nessun redirect (un redirect potrebbe portare il token altrove), niente cache. */
object UrlConnectionTransport : Transport {
    override fun send(method: String, url: String, headers: Map<String, String>, body: String?): HttpReply {
        val connection = URL(url).openConnection() as HttpURLConnection
        connection.instanceFollowRedirects = false
        connection.useCaches = false
        connection.connectTimeout = 15_000
        connection.readTimeout = 150_000
        connection.requestMethod = method
        headers.forEach { (k, v) -> connection.setRequestProperty(k, v) }
        if (body != null) {
            connection.doOutput = true
            connection.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
        }
        val status = connection.responseCode
        val stream = if (status in 200..299) connection.inputStream else connection.errorStream
        val text = stream?.bufferedReader(Charsets.UTF_8)?.use { it.readText() } ?: ""
        connection.disconnect()
        return HttpReply(status, text)
    }
}

/** Client Android del Core: pairing HTTPS, scope limitati e revoca immediata. */
class CoreClient(private val transport: Transport = UrlConnectionTransport) {

    companion object {
        /** Solo HTTPS, senza credenziali, query, frammenti o percorsi: l'indirizzo è quello del proxy fidato. */
        fun origin(value: String): String {
            val uri = try { URI(value.trim()) } catch (e: Exception) { throw CoreException.InvalidOrigin() }
            val host = uri.host
            if (uri.scheme?.lowercase() != "https" || host.isNullOrEmpty() || uri.userInfo != null ||
                uri.query != null || uri.fragment != null || !(uri.path.isNullOrEmpty() || uri.path == "/")
            ) throw CoreException.InvalidOrigin()
            val port = if (uri.port == -1) "" else ":${uri.port}"
            return "https://${host.lowercase()}$port"
        }

        private val HEX_TOKEN = Regex("^[0-9a-f]{64}$")
    }

    fun pair(origin: String, code: String, name: String): DeviceSession {
        val base = origin(origin)
        val body = JSONObject().put("code", code.trim()).put("name", name.trim()).put("platform", "android")
        val result = request(base, "/api/devices/pair", token = null, body = body)
        val token = result.optString("token")
        val scopes = result.optJSONObject("device")?.optJSONArray("scopes").strings()
        if (!HEX_TOKEN.matches(token) || "chat" !in scopes) throw CoreException.InvalidPairing()
        return DeviceSession(base, token, scopes)
    }

    fun history(session: DeviceSession): List<ChatMessage> =
        request(session.origin, "/api/conversation?limit=50", session.token).items().map {
            ChatMessage(it.optString("role"), it.optString("content"))
        }

    fun pending(session: DeviceSession): List<PendingAction> =
        request(session.origin, "/api/confirmations", session.token).items().map {
            PendingAction(it.optString("confirmation_id"), it.optString("target"), it.optString("expected_effect"), it.optString("expires_at"))
        }

    /** Legge messaggi e conferme nello stesso snapshot del Core. */
    fun syncChat(session: DeviceSession): Pair<List<ChatMessage>, List<PendingAction>> {
        val snapshot = try {
            request(session.origin, "/api/chat/sync", session.token)
        } catch (error: CoreException.Rejected) {
            if (error.status == 404) return history(session) to pending(session)
            throw error
        }
        val messages = snapshot.optJSONArray("messages") ?: throw CoreException.InvalidResponse()
        val confirmations = snapshot.optJSONArray("confirmations") ?: throw CoreException.InvalidResponse()
        return (0 until messages.length()).map { index ->
            val item = messages.getJSONObject(index)
            ChatMessage(item.optString("role"), item.optString("content"))
        } to (0 until confirmations.length()).map { index ->
            val item = confirmations.getJSONObject(index)
            PendingAction(item.optString("confirmation_id"), item.optString("target"), item.optString("expected_effect"), item.optString("expires_at"))
        }
    }

    /** Attività recenti con il loro stato (permesso `chat`). */
    fun tasks(session: DeviceSession): List<TaskSummary> =
        request(session.origin, "/api/tasks?limit=30", session.token).items().map {
            TaskSummary(it.optString("task_id"), it.optString("objective"), it.optString("state"), it.optString("created_at"))
        }

    /** Notifiche con il conteggio delle non lette (RF-13). */
    fun notices(session: DeviceSession): Notices {
        val json = request(session.origin, "/api/notifications?limit=30", session.token)
        val items = json.items().map {
            Notice(
                it.optString("notification_id"),
                it.optString("severity"),
                it.optString("category"),
                it.optString("title"),
                it.optString("body"),
                it.optString("created_at"),
                !it.isNull("read_at") && it.optString("read_at").isNotEmpty(),
                it.optJSONObject("link")?.let { link ->
                    val view = link.optString("view").takeIf { target ->
                        target in notificationViews
                    }
                    view?.let {
                        NotificationLink(
                            it,
                            link.optString("ref").takeIf { ref -> ref.isNotEmpty() && ref.length <= 200 },
                        )
                    }
                },
            )
        }
        return Notices(json.optInt("unread", 0), items)
    }

    fun markRead(id: String, session: DeviceSession) {
        if (!Regex("^[0-9a-fA-F-]{36}$").matches(id)) throw CoreException.InvalidResponse()
        request(session.origin, "/api/notifications/${id.lowercase()}/read", session.token, JSONObject())
    }

    fun markAllRead(session: DeviceSession): Int =
        request(session.origin, "/api/notifications/read-all", session.token, JSONObject()).optInt("unread", 0)

    /** Registra il token FCM senza inserirlo nell'URL o nei log. */
    fun registerPush(token: String, session: DeviceSession) {
        if (token.isBlank() || token.length > 4096 || token.any(Char::isWhitespace) || token.any(Char::isISOControl)) {
            throw CoreException.InvalidPushToken()
        }
        request(
            session.origin,
            "/api/push/subscriptions",
            session.token,
            JSONObject().put("token", token).put("platform", "android"),
        )
    }

    fun removePush(session: DeviceSession) {
        request(session.origin, "/api/push/subscriptions", session.token, methodOverride = "DELETE")
    }

    fun send(message: String, session: DeviceSession): String? =
        request(session.origin, "/api/cloud/chat", session.token, JSONObject().put("message", message).put("provider", "auto")).optString("reply").ifEmpty { null }

    fun decide(action: PendingAction, approve: Boolean, session: DeviceSession): String? {
        if (!session.canConfirm) throw CoreException.ScopeRequired()
        val id = action.confirmationId.lowercase()
        if (!Regex("^[0-9a-f-]{36}$").matches(id)) throw CoreException.InvalidResponse()
        return request(session.origin, "/api/confirmations/$id", session.token, JSONObject().put("approve", approve))
            .optString("reply").ifEmpty { null }
    }

    private fun request(
        origin: String,
        path: String,
        token: String?,
        body: JSONObject? = null,
        methodOverride: String? = null,
    ): JSONObject {
        val headers = mutableMapOf("Accept" to "application/json")
        if (token != null) headers["X-Jarvis-Device"] = token
        if (body != null) headers["Content-Type"] = "application/json"
        val method = methodOverride ?: if (body == null) "GET" else "POST"
        val reply = transport.send(method, origin(origin) + path, headers, body?.toString())
        if (reply.status == 401 && token != null) throw CoreException.Revoked()
        val json = try { JSONObject(reply.body.ifEmpty { "{}" }) } catch (e: Exception) { null }
        if (reply.status !in 200..299) {
            throw CoreException.Rejected(reply.status, json?.optJSONObject("error")?.optString("message")?.ifEmpty { null })
        }
        return json ?: throw CoreException.InvalidResponse()
    }

    private fun JSONObject.items(): List<JSONObject> {
        val array = optJSONArray("items") ?: throw CoreException.InvalidResponse()
        return (0 until array.length()).mapNotNull { array.optJSONObject(it) }
    }
}

private fun JSONArray?.strings(): List<String> = if (this == null) emptyList() else (0 until length()).map { optString(it) }
