package local.jarvis.client

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PairingLinkTest {
    @Test fun aLinkFillsOriginAndCode() {
        val parsed = PairingLink.parse("jarvis://pair?origin=https%3A%2F%2Fcasa.tailnet.ts.net&code=ab12cd34")
        assertEquals(PairingLink("https://casa.tailnet.ts.net", "AB12CD34"), parsed)
    }

    @Test fun aLinkInsideAMessageIsFound() {
        val text = "Ciao, apri questo su Android:\njarvis://pair?origin=https%3A%2F%2Fcore.example.net%3A8443%2F&code=K7M2P9QX\nGrazie!"
        assertEquals(PairingLink("https://core.example.net:8443", "K7M2P9QX"), PairingLink.parse(text))
    }

    @Test fun unsafeOrIncompleteLinksAreRefused() {
        for (bad in listOf(
            null, "", "https://casa.example.net",
            "jarvis://pair?origin=http%3A%2F%2Fcasa.example.net&code=AB12CD34",
            "jarvis://pair?origin=https%3A%2F%2Futente%40casa.example.net&code=AB12CD34",
            "jarvis://pair?origin=https%3A%2F%2Fcasa.example.net%2Fpercorso&code=AB12CD34",
            "jarvis://pair?origin=https%3A%2F%2Flocalhost&code=AB12CD34",
            "jarvis://pair?origin=https%3A%2F%2Fcasa.example.net&code=",
            "jarvis://pair?origin=https%3A%2F%2Fcasa.example.net&code=AB%2012",
            "jarvis://pair?code=AB12CD34",
        )) {
            assertNull("$bad", PairingLink.parse(bad))
        }
    }
}
