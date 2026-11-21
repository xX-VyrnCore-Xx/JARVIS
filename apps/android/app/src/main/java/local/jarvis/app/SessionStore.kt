package local.jarvis.app

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import local.jarvis.client.DeviceSession
import org.json.JSONArray
import org.json.JSONObject
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Sessione del dispositivo cifrata con AES-GCM e una chiave dell'Android Keystore, che non lascia il
 * telefono. Il file delle preferenze contiene solo il testo cifrato; il backup dell'app è disattivato.
 */
class SessionStore(context: Context) {
    private val prefs = context.getSharedPreferences("jarvis-session", Context.MODE_PRIVATE)
    private val alias = "jarvis-session-key"

    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build()
        )
        return generator.generateKey()
    }

    fun save(session: DeviceSession) {
        val plain = JSONObject()
            .put("origin", session.origin)
            .put("token", session.token)
            .put("scopes", JSONArray(session.scopes))
            .toString()
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val sealed = cipher.doFinal(plain.toByteArray(Charsets.UTF_8))
        prefs.edit()
            .putString("iv", Base64.encodeToString(cipher.iv, Base64.NO_WRAP))
            .putString("data", Base64.encodeToString(sealed, Base64.NO_WRAP))
            .apply()
    }

    fun load(): DeviceSession? = try {
        val iv = Base64.decode(prefs.getString("iv", null) ?: return null, Base64.NO_WRAP)
        val data = Base64.decode(prefs.getString("data", null) ?: return null, Base64.NO_WRAP)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv)) }
        val json = JSONObject(String(cipher.doFinal(data), Charsets.UTF_8))
        val scopes = json.getJSONArray("scopes").let { a -> (0 until a.length()).map { a.getString(it) } }
        DeviceSession(json.getString("origin"), json.getString("token"), scopes)
    } catch (e: Exception) {
        clear()
        null
    }

    fun pushEnabled(): Boolean = prefs.getBoolean("push-enabled", false)

    fun setPushEnabled(enabled: Boolean) {
        prefs.edit().putBoolean("push-enabled", enabled).apply()
    }

    fun clear() {
        prefs.edit().clear().apply()
    }
}
