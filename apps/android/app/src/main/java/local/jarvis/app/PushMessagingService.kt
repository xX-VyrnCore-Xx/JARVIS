package local.jarvis.app

import android.Manifest
import android.app.PendingIntent
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import local.jarvis.client.CoreClient

class PushMessagingService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        if (!JarvisApplication.firebaseConfigured()) return
        CoroutineScope(Dispatchers.IO).launch {
            val store = SessionStore(applicationContext)
            if (!store.pushEnabled()) return@launch
            val session = store.load() ?: return@launch
            runCatching { CoreClient().registerPush(token, session) }
                .onFailure { Log.w(TAG, "Il Core non ha accettato il nuovo token push (${it.javaClass.simpleName}).") }
        }
    }

    override fun onMessageReceived(message: RemoteMessage) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return

        val title = message.notification?.title ?: message.data["title"] ?: "JARVIS"
        val body = message.notification?.body ?: "Nuova notifica"
        val launch = packageManager.getLaunchIntentForPackage(packageName) ?: return
        val pending = PendingIntent.getActivity(
            this,
            0,
            launch,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_jarvis)
            .setContentTitle(title)
            .setContentText(body)
            .setContentIntent(pending)
            .setAutoCancel(true)
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .build()
        val id = message.data["notification_id"]?.hashCode() ?: System.currentTimeMillis().toInt()
        NotificationManagerCompat.from(this).notify(id, notification)
    }

    companion object {
        const val CHANNEL_ID = "jarvis_notifications"
        private const val TAG = "JarvisPush"
    }
}
