package local.jarvis.client

import java.net.URLDecoder

/**
 * Collegamento di associazione (ADR-0137): `jarvis://pair?origin=https%3A%2F%2Fcasa.esempio.net&code=AB12CD34`.
 * Un solo tocco compila indirizzo e codice, senza digitare nulla. Il Core lo genera, il proprietario lo condivide
 * (messaggio, AirDrop, appunti) e l'app lo apre o lo legge dagli appunti.
 */
data class PairingLink(val origin: String, val code: String) {
    companion object {
        private val link = Regex("""jarvis://pair\?[^\s"'<>]+""", RegexOption.IGNORE_CASE)
        private val originShape = Regex("""https://[A-Za-z0-9]([A-Za-z0-9-]*[A-Za-z0-9])?(\.[A-Za-z0-9]([A-Za-z0-9-]*[A-Za-z0-9])?)+(:\d{1,5})?""")
        private val codeShape = Regex("""[A-Z0-9]{4,16}""")

        /** Estrae il collegamento anche da un testo che lo contiene (messaggio incollato). */
        fun parse(text: String?): PairingLink? {
            val found = link.find(text.orEmpty())?.value ?: return null
            val query = found.substringAfter('?').substringBefore('#')
            val params = query.split('&').mapNotNull { part ->
                val at = part.indexOf('=')
                if (at <= 0) null else part.substring(0, at) to runCatching { URLDecoder.decode(part.substring(at + 1), "UTF-8") }.getOrNull()
            }.toMap()
            val origin = params["origin"]?.trim()?.trimEnd('/') ?: return null
            val code = params["code"]?.trim()?.uppercase() ?: return null
            if (!originShape.matches(origin) || !codeShape.matches(code)) return null
            return PairingLink(origin, code)
        }
    }
}
