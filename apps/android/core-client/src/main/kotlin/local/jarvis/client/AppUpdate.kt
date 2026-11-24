package local.jarvis.client

import org.json.JSONArray
import java.io.File
import java.net.URI
import java.security.MessageDigest

/** Release dell'app Android disponibile su GitHub (ADR-0112). */
data class AppRelease(
    val version: String,
    val apkName: String,
    val apkUrl: String,
    val apkSize: Long,
    val sumsUrl: String,
    val pageUrl: String,
)

/**
 * Aggiornamento dell'app dall'interno: trova la release, scarica l'APK solo da GitHub via HTTPS e ne verifica il
 * SHA-256 con `SHA256SUMS`. L'installazione resta al sistema, che accetta solo un APK firmato con la stessa chiave.
 */
object AppUpdate {
    const val RELEASES_URL = "https://api.github.com/repos/xX-VyrnCore-Xx/JARVIS/releases?per_page=15"
    private const val MAX_APK_BYTES = 200L * 1024 * 1024
    private const val MAX_SMALL_BYTES = 2L * 1024 * 1024

    private fun parts(version: String): Pair<List<Int>, String>? {
        val core = version.trim().removePrefix("v")
        val numbers = core.substringBefore('-').substringBefore('+').split('.')
        if (numbers.size < 2 || numbers.any { it.isEmpty() || !it.all(Char::isDigit) }) return null
        return numbers.map { it.toInt() } to core.substringAfter('-', "")
    }

    /** Vero se `candidate` è più recente di `current` (una prerelease precede la stabile con gli stessi numeri). */
    fun isNewer(candidate: String, current: String): Boolean {
        val (a, preA) = parts(candidate) ?: return false
        val (b, preB) = parts(current) ?: return false
        for (i in 0 until maxOf(a.size, b.size)) {
            val x = a.getOrElse(i) { 0 }
            val y = b.getOrElse(i) { 0 }
            if (x != y) return x > y
        }
        return when {
            preA == preB -> false
            preA.isEmpty() -> true
            preB.isEmpty() -> false
            else -> preA > preB
        }
    }

    /** La release più recente, più nuova di `current`, con l'APK e `SHA256SUMS`. Bozze e prerelease escluse. */
    fun latest(releasesJson: String, current: String): AppRelease? {
        val releases = JSONArray(releasesJson)
        var best: AppRelease? = null
        for (i in 0 until releases.length()) {
            val r = releases.optJSONObject(i) ?: continue
            if (r.optBoolean("draft") || r.optBoolean("prerelease")) continue
            val tag = r.optString("tag_name")
            if (parts(tag) == null || !isNewer(tag, current)) continue
            val version = tag.removePrefix("v")
            val assets = r.optJSONArray("assets") ?: continue
            val find = { name: String ->
                (0 until assets.length()).mapNotNull { assets.optJSONObject(it) }.firstOrNull { it.optString("name") == name }
            }
            val apkName = "JARVIS_${version}_android-release.apk"
            val apk = find(apkName) ?: continue
            val sums = find("SHA256SUMS") ?: continue
            val release = AppRelease(
                version, apkName, apk.optString("browser_download_url"), apk.optLong("size"),
                sums.optString("browser_download_url"), r.optString("html_url"),
            )
            if (!allowedUrl(release.apkUrl) || !allowedUrl(release.sumsUrl)) continue
            if (best == null || isNewer(release.version, best.version)) best = release
        }
        return best
    }

    /** Righe `<sha256>  <nome>` di `sha256sum`. */
    fun parseSums(text: String): Map<String, String> =
        text.lines().mapNotNull { line ->
            val hash = line.trim().substringBefore(' ')
            val name = line.trim().substringAfter(' ', "").trim().removePrefix("*")
            if (hash.length == 64 && hash.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' } && name.isNotEmpty()) {
                name to hash.lowercase()
            } else null
        }.toMap()

    /** Solo HTTPS verso GitHub, senza credenziali nell'indirizzo. */
    fun allowedUrl(url: String): Boolean {
        val uri = runCatching { URI(url) }.getOrNull() ?: return false
        val host = uri.host?.lowercase() ?: return false
        return uri.scheme.equals("https", ignoreCase = true) && uri.userInfo == null &&
            (host == "api.github.com" || host == "github.com" || host.endsWith(".githubusercontent.com"))
    }

    fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(1 shl 16)
            while (true) {
                val n = input.read(buffer)
                if (n < 0) break
                digest.update(buffer, 0, n)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    /** Apre l'indirizzo seguendo a mano i reindirizzamenti, ciascuno riconvalidato. */
    private fun open(url: String, accept: String): java.net.HttpURLConnection {
        var current = url
        repeat(5) {
            require(allowedUrl(current)) { "indirizzo non ammesso" }
            val connection = URI.create(current).toURL().openConnection() as javax.net.ssl.HttpsURLConnection
            connection.instanceFollowRedirects = false
            connection.useCaches = false
            connection.connectTimeout = 15_000
            connection.readTimeout = 60_000
            connection.setRequestProperty("Accept", accept)
            connection.setRequestProperty("User-Agent", "JARVIS-Android")
            val status = connection.responseCode
            if (status in 300..399) {
                val next = connection.getHeaderField("Location") ?: throw IllegalStateException("reindirizzamento senza destinazione")
                current = URI.create(current).resolve(next).toString()
                connection.disconnect()
                return@repeat
            }
            check(status in 200..299) { "GitHub ha risposto HTTP $status" }
            return connection
        }
        throw IllegalStateException("troppi reindirizzamenti")
    }

    private fun readText(url: String, accept: String): String {
        val connection = open(url, accept)
        try {
            return connection.inputStream.use { stream ->
                val out = java.io.ByteArrayOutputStream()
                val buffer = ByteArray(8192)
                while (true) {
                    val n = stream.read(buffer)
                    if (n < 0) break
                    check(out.size() + n <= MAX_SMALL_BYTES) { "risposta troppo grande" }
                    out.write(buffer, 0, n)
                }
                out.toString(Charsets.UTF_8.name())
            }
        } finally {
            connection.disconnect()
        }
    }

    /** Controlla GitHub; `null` se non c'è nulla di più recente. */
    fun check(current: String): AppRelease? = latest(readText(RELEASES_URL, "application/vnd.github+json"), current)

    /** Scarica e verifica l'APK in `dir`; restituisce il file solo se il SHA-256 coincide con `SHA256SUMS`. */
    fun download(release: AppRelease, dir: File, onProgress: (Long, Long) -> Unit): File {
        val expected = parseSums(readText(release.sumsUrl, "text/plain"))[release.apkName]
            ?: throw IllegalStateException("SHA256SUMS non contiene l'APK: aggiornamento rifiutato")
        dir.mkdirs()
        dir.listFiles()?.forEach { it.delete() }
        val part = File(dir, release.apkName + ".part")
        val connection = open(release.apkUrl, "application/octet-stream")
        try {
            val total = connection.contentLengthLong.takeIf { it > 0 } ?: release.apkSize
            check(total <= MAX_APK_BYTES) { "il file supera il limite" }
            var written = 0L
            connection.inputStream.use { input ->
                part.outputStream().use { output ->
                    val buffer = ByteArray(1 shl 16)
                    while (true) {
                        val n = input.read(buffer)
                        if (n < 0) break
                        written += n
                        check(written <= MAX_APK_BYTES) { "il file supera il limite" }
                        output.write(buffer, 0, n)
                        onProgress(written, total)
                    }
                }
            }
        } finally {
            connection.disconnect()
        }
        if (sha256(part) != expected) {
            part.delete()
            throw IllegalStateException("la verifica SHA-256 è fallita: file scartato")
        }
        val ready = File(dir, release.apkName)
        check(part.renameTo(ready)) { "file verificato non salvabile" }
        return ready
    }
}
