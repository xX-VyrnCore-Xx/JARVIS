package local.jarvis.client

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** Verifiche del client con un trasporto sintetico. */
class CoreClientTest {
    private val token = "a".repeat(64)

    private class Recorder(private val replies: List<HttpReply>) : Transport {
        val calls = mutableListOf<Triple<String, String, Map<String, String>>>()
        val bodies = mutableListOf<String?>()
        override fun send(method: String, url: String, headers: Map<String, String>, body: String?): HttpReply {
            calls += Triple(method, url, headers)
            bodies += body
            return replies[calls.size - 1]
        }
    }

    @Test
    fun originAcceptsOnlyBareHttps() {
        assertEquals("https://jarvis.esempio.ts.net", CoreClient.origin(" https://JARVIS.esempio.ts.net/ "))
        assertEquals("https://host:8443", CoreClient.origin("https://host:8443"))
        assertEquals("https://host", CoreClient.origin("https://host:443"))
        for (bad in listOf("http://jarvis.example", "https://user:pw@jarvis.example", "https://jarvis.example/api",
            "https://jarvis.example:0", "https://jarvis.example:65536", "https://jarvis.example:",
            "https://jarvis.example?x=1", "https://jarvis.example#f", "jarvis.example", "")) {
            assertThrows(bad, CoreException.InvalidOrigin::class.java) { CoreClient.origin(bad) }
        }
    }

    @Test
    fun connectivityIsUnauthenticatedAndExpectsTheCoreProtocol() {
        val transport = Recorder(listOf(HttpReply(200, """{"service":"jarvis-core","protocol":1}""")))
        CoreClient(transport).checkConnectivity("https://jarvis.example")
        val (method, url, headers) = transport.calls.single()
        assertEquals("GET" to "https://jarvis.example/api/connectivity", method to url)
        assertFalse(headers.containsKey("X-Jarvis-Device"))
        assertThrows(CoreException.InvalidResponse::class.java) {
            CoreClient(Recorder(listOf(HttpReply(200, "{}")))).checkConnectivity("https://jarvis.example")
        }
        assertThrows(CoreException.InvalidOrigin::class.java) {
            UrlConnectionTransport.send("GET", "http://jarvis.example", emptyMap(), null)
        }
    }

    @Test
    fun pairingSendsAndroidPlatformAndValidatesTheToken() {
        val t = Recorder(listOf(HttpReply(200, """{"token":"$token","device":{"scopes":["chat","confirm"]}}""")))
        val session = CoreClient(t).pair("https://jarvis.example", " ABCD-EFGH ", "Pixel")
        assertEquals(DeviceSession("https://jarvis.example", token, listOf("chat", "confirm")), session)
        assertTrue(session.canConfirm)
        val (method, url, headers) = t.calls.single()
        assertEquals("POST" to "https://jarvis.example/api/devices/pair", method to url)
        assertFalse("nessun token prima dell'associazione", headers.containsKey("X-Jarvis-Device"))
        assertTrue(t.bodies.single()!!.contains("\"platform\":\"android\""))

        for (bad in listOf("""{"token":"corto","device":{"scopes":["chat"]}}""",
            """{"token":"$token","device":{"scopes":["node"]}}""")) {
            assertThrows(CoreException.InvalidPairing::class.java) {
                CoreClient(Recorder(listOf(HttpReply(200, bad)))).pair("https://jarvis.example", "x", "y")
            }
        }
    }

    @Test
    fun chatHistoryAndPendingUseTheDeviceToken() {
        val t = Recorder(listOf(
            HttpReply(200, """{"items":[{"role":"user","content":"ciao"},{"role":"assistant","content":"Ciao!"}]}"""),
            HttpReply(200, """{"reply":"Sono le 10:00"}"""),
            HttpReply(200, """{"items":[{"confirmation_id":"6F1C2D3E-4B5A-4C6D-8E7F-901234567890","target":"jarvis://file/workspace/a.txt","expected_effect":"Scrivere","expires_at":"2026-11-18T10:00:00Z"}]}"""),
        ))
        val client = CoreClient(t)
        val session = DeviceSession("https://jarvis.example", token, listOf("chat"))
        assertEquals(listOf(ChatMessage("user", "ciao"), ChatMessage("assistant", "Ciao!")), client.history(session))
        assertEquals("Sono le 10:00", client.send("che ore sono", session))
        val action = client.pending(session).single()
        assertEquals("Scrivere", action.expectedEffect)
        assertTrue(t.calls.all { it.third["X-Jarvis-Device"] == token })
        assertEquals("https://jarvis.example/api/conversation?limit=50", t.calls[0].second)
        // Senza il permesso «confirm» la decisione non parte nemmeno.
        assertThrows(CoreException.ScopeRequired::class.java) { client.decide(action, true, session) }
        assertEquals(3, t.calls.size)
    }

    @Test
    fun chatSyncUsesOneAuthenticatedSnapshotAndSupportsOlderCores() {
        val session = DeviceSession("https://jarvis.example", token, listOf("chat"))
        val snapshot = Recorder(listOf(HttpReply(200, """{"messages":[{"role":"user","content":"ciao"}],"confirmations":[]}""")))
        assertEquals(listOf(ChatMessage("user", "ciao")) to emptyList<PendingAction>(), CoreClient(snapshot).syncChat(session))
        assertEquals("https://jarvis.example/api/chat/sync", snapshot.calls.single().second)
        assertEquals(token, snapshot.calls.single().third["X-Jarvis-Device"])

        val old = Recorder(listOf(HttpReply(404, "{}"), HttpReply(200, """{"items":[]}"""), HttpReply(200, """{"items":[]}""")))
        assertEquals(emptyList<ChatMessage>() to emptyList<PendingAction>(), CoreClient(old).syncChat(session))
        assertEquals(3, old.calls.size)
    }

    @Test
    fun decisionUsesLowercaseIdAndRevocationIsReported() {
        val t = Recorder(listOf(HttpReply(200, """{"reply":null}"""), HttpReply(401, """{"error":{"message":"revocato"}}""")))
        val client = CoreClient(t)
        val session = DeviceSession("https://jarvis.example", token, listOf("chat", "confirm"))
        val action = PendingAction("6F1C2D3E-4B5A-4C6D-8E7F-901234567890", "t", "e", "x")
        assertNull(client.decide(action, false, session))
        assertEquals("https://jarvis.example/api/confirmations/6f1c2d3e-4b5a-4c6d-8e7f-901234567890", t.calls[0].second)
        assertTrue(t.bodies[0]!!.contains("\"approve\":false"))
        assertThrows(CoreException.Revoked::class.java) { client.history(session) }
    }

    @Test
    fun errorsCarryTheCoreMessage() {
        val t = Recorder(listOf(HttpReply(403, """{"error":{"code":"owner_only","message":"solo dal computer del proprietario"}}""")))
        val e = assertThrows(CoreException.Rejected::class.java) {
            CoreClient(t).send("x", DeviceSession("https://jarvis.example", token, listOf("chat")))
        }
        assertEquals(403, e.status)
        assertEquals("solo dal computer del proprietario", e.message)
    }

    @Test
    fun tasksAndNoticesUseDeviceRoutesOnly() {
        val t = Recorder(listOf(
            HttpReply(200, """{"items":[{"task_id":"t1","objective":"che ore sono","state":"succeeded","created_at":"2026-11-18T10:00:00Z"}]}"""),
            HttpReply(200, """{"unread":1,"items":[{"notification_id":"n1","severity":"warning","category":"confirmation","title":"Conferma richiesta","body":"x","created_at":"2026-11-18T10:00:00Z","read_at":null,"link":{"view":"tasks","ref":"task-1"}},{"notification_id":"n2","severity":"info","category":"system","title":"Fatto","created_at":"2026-11-18T09:00:00Z","read_at":"2026-11-18T09:01:00Z","link":{"view":"untrusted","ref":"../devices"}}]}"""),
            HttpReply(200, """{"unread":0}"""),
        ))
        val client = CoreClient(t)
        val session = DeviceSession("https://jarvis.example", token, listOf("chat"))
        assertEquals(listOf(TaskSummary("t1", "che ore sono", "succeeded", "2026-11-18T10:00:00Z")), client.tasks(session))
        val notices = client.notices(session)
        assertEquals(1, notices.unread)
        assertEquals(listOf(false, true), notices.items.map { it.read })
        assertEquals("warning", notices.items.first().severity)
        assertEquals("system", notices.items.last().category)
        assertEquals(NotificationLink("tasks", "task-1"), notices.items.first().link)
        assertNull(notices.items.last().link)
        assertEquals(0, client.markAllRead(session))
        assertEquals(listOf("/api/tasks?limit=30", "/api/notifications?limit=30", "/api/notifications/read-all"),
            t.calls.map { it.second.removePrefix("https://jarvis.example") })
        assertTrue(t.calls.all { it.third["X-Jarvis-Device"] == token })
    }

    @Test
    fun individualNotificationReadUsesValidatedDeviceRoute() {
        val id = "6F1C2D3E-4B5A-4C6D-8E7F-901234567890"
        val t = Recorder(listOf(HttpReply(200, """{"read":true}""")))
        val client = CoreClient(t)
        val session = DeviceSession("https://jarvis.example", token, listOf("chat"))

        client.markRead(id, session)

        assertEquals("POST", t.calls.single().first)
        assertEquals(
            "https://jarvis.example/api/notifications/${id.lowercase()}/read",
            t.calls.single().second,
        )
        assertEquals(token, t.calls.single().third["X-Jarvis-Device"])
        assertThrows(CoreException.InvalidResponse::class.java) {
            client.markRead("../devices", session)
        }
        assertEquals(1, t.calls.size)
    }

    @Test
    fun pushRegistrationUsesAuthenticatedDeviceRoutesAndRejectsMalformedTokens() {
        val t = Recorder(listOf(HttpReply(200, """{"registered":true}"""), HttpReply(200, """{"deleted":true}""")))
        val client = CoreClient(t)
        val session = DeviceSession("https://jarvis.example", token, listOf("chat"))
        client.registerPush("fcm-token_123:ABC", session)
        client.removePush(session)

        assertEquals(
            listOf(
                "POST" to "https://jarvis.example/api/push/subscriptions",
                "DELETE" to "https://jarvis.example/api/push/subscriptions",
            ),
            t.calls.map { it.first to it.second },
        )
        assertTrue(t.calls.all { it.third["X-Jarvis-Device"] == token })
        assertTrue(t.bodies[0]!!.contains(""""platform":"android""""))
        assertNull(t.bodies[1])
        for (bad in listOf("", "has whitespace", "x\nbad", "x".repeat(4097))) {
            assertThrows(CoreException.InvalidPushToken::class.java) { client.registerPush(bad, session) }
        }
        assertEquals(2, t.calls.size)
    }
}
