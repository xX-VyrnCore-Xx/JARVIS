package local.jarvis.client

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class AppUpdateTest {
    private fun asset(name: String, id: Int) =
        """{"name":"$name","size":10,"browser_download_url":"https://github.com/o/r/releases/download/v1/$name"}"""

    private fun release(tag: String, draft: Boolean = false, pre: Boolean = false, apk: Boolean = true) =
        """{"tag_name":"$tag","draft":$draft,"prerelease":$pre,"html_url":"https://github.com/o/r/releases/tag/$tag","assets":[
            ${if (apk) asset("JARVIS_${tag.removePrefix("v")}_android-release.apk", 1) + "," else ""}${asset("SHA256SUMS", 2)}]}"""

    @Test fun versionsAreComparedNumerically() {
        assertTrue(AppUpdate.isNewer("v1.13.291", "1.13.290"))
        assertTrue(AppUpdate.isNewer("1.14.0", "1.13.999"))
        assertFalse(AppUpdate.isNewer("1.13.290", "1.13.290"))
        assertFalse(AppUpdate.isNewer("1.9.0", "1.13.0"))
        assertTrue(AppUpdate.isNewer("1.3.0", "1.3.0-rc.1"))
        assertFalse(AppUpdate.isNewer("1.3.0-rc.1", "1.3.0"))
        assertFalse(AppUpdate.isNewer("boh", "1.0.0"))
        assertFalse(AppUpdate.isNewer("999999999999.1.1", "1.0.0"))
    }

    @Test fun theNewestStableReleaseWithAnApkIsChosen() {
        val json = "[" + listOf(
            release("v1.13.300", draft = true),
            release("v1.13.295", pre = true),
            release("v1.13.294", apk = false),
            release("v1.13.292"),
            release("v1.13.291"),
        ).joinToString(",") + "]"
        val found = AppUpdate.latest(json, "1.13.290")!!
        assertEquals("1.13.292", found.version)
        assertEquals("JARVIS_1.13.292_android-release.apk", found.apkName)
        assertNull(AppUpdate.latest(json, "1.13.292"))
    }

    @Test fun onlyHttpsGithubHostsAreAllowed() {
        assertTrue(AppUpdate.allowedUrl("https://github.com/o/r/releases/download/v1/x"))
        assertTrue(AppUpdate.allowedUrl("https://release-assets.githubusercontent.com/a"))
        for (bad in listOf("http://github.com/x", "https://evilgithub.com/x", "https://github.com.evil.org/x",
            "https://u:p@github.com/x", "https://github.com:8443/x", "https://fixture.githubusercontent.com/x", "https://example.com/x", "file:///etc/passwd")) {
            assertFalse(bad, AppUpdate.allowedUrl(bad))
        }
    }

    @Test fun checksumsAreParsedAndFilesHashed() {
        val h = "a".repeat(64)
        val sums = AppUpdate.parseSums("$h  one.apk\n$h *two.apk\nnope  bad\n")
        assertEquals(setOf("one.apk", "two.apk"), sums.keys)
        val conflicting = "b".repeat(64)
        assertFalse(AppUpdate.parseSums("$h  one.apk\n$conflicting  one.apk").containsKey("one.apk"))
        assertEquals(h, AppUpdate.parseSums("$h  one.apk\n$h *one.apk")["one.apk"])
        val file = File.createTempFile("jarvis", ".bin")
        file.writeText("abc")
        assertEquals("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", AppUpdate.sha256(file))
        file.delete()
    }
}
