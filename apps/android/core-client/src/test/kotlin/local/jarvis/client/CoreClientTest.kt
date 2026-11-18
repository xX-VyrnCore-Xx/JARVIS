package local.jarvis.client

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** Verifiche del client con un trasporto sintetico (stessi casi del client iOS). */
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
        for (bad in listOf("http://jarvis.example", "https://user:pw@jarvis.example", "https://jarvis.example/api",
            "https://jarvis.example?x=1", "https://jarvis.example#f", "jarvis.example", "")) {
            assertThrows(bad, CoreException.InvalidOrigin::class.java) { CoreClient.origin(bad) }
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
}
