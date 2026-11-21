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

data class Notice(val id: String, val severity: String, val title: String, val body: String, val createdAt: String, val read: Boolean)

data class Notices(val unread: Int, val items: List<Notice>)

data class PendingAction(val confirmationId: String, val target: String, val expectedEffect: String, val expiresAt: String)

sealed class CoreException(message: String) : Exception(message) {
    class InvalidOrigin : CoreException("Inserisci l'indirizzo HTTPS del Core, ad esempio https://jarvis.example.")
    class InvalidResponse : CoreException("Il Core ha restituito una risposta inattesa.")
    class Revoked : CoreException("Dispositivo revocato. Genera un nuovo codice per associarlo.")
    class ScopeRequired : CoreException("Questo dispositivo non può confermare azioni.")
    class InvalidPairing : CoreException("La risposta di associazione non contiene un token utilizzabile.")
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

/** Client del Core per il telefono: stesse regole del client iOS (ADR-0018). */
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

    /** Attività recenti con il loro stato (permesso `chat`). */
    fun tasks(session: DeviceSession): List<TaskSummary> =
        request(session.origin, "/api/tasks?limit=30", session.token).items().map {
            TaskSummary(it.optString("task_id"), it.optString("objective"), it.optString("state"), it.optString("created_at"))
        }

    /** Notifiche con il conteggio delle non lette (RF-13). */
    fun notices(session: DeviceSession): Notices {
        val json = request(session.origin, "/api/notifications?limit=30", session.token)
        val items = json.items().map {
            Notice(it.optString("notification_id"), it.optString("severity"), it.optString("title"),
                it.optString("body"), it.optString("created_at"), !it.isNull("read_at") && it.optString("read_at").isNotEmpty())
        }
        return Notices(json.optInt("unread", 0), items)
    }

    fun markAllRead(session: DeviceSession): Int =
        request(session.origin, "/api/notifications/read-all", session.token, JSONObject()).optInt("unread", 0)

    fun send(message: String, session: DeviceSession): String? =
        request(session.origin, "/api/chat", session.token, JSONObject().put("message", message)).optString("reply").ifEmpty { null }

    fun decide(action: PendingAction, approve: Boolean, session: DeviceSession): String? {
        if (!session.canConfirm) throw CoreException.ScopeRequired()
        val id = action.confirmationId.lowercase()
        if (!Regex("^[0-9a-f-]{36}$").matches(id)) throw CoreException.InvalidResponse()
        return request(session.origin, "/api/confirmations/$id", session.token, JSONObject().put("approve", approve))
            .optString("reply").ifEmpty { null }
    }

    private fun request(origin: String, path: String, token: String?, body: JSONObject? = null): JSONObject {
        val headers = mutableMapOf("Accept" to "application/json")
        if (token != null) headers["X-Jarvis-Device"] = token
        if (body != null) headers["Content-Type"] = "application/json"
        val reply = transport.send(if (body == null) "GET" else "POST", origin(origin) + path, headers, body?.toString())
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
