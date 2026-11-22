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
        val store = SessionStore(applicationContext)
        if (!store.pushEnabled() || !NotificationManagerCompat.from(this).areNotificationsEnabled()) return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return

        val title = message.notification?.title ?: message.data["title"] ?: "JARVIS"
        val body = message.notification?.body ?: message.data["body"] ?: "Apri JARVIS per i dettagli."
        val launch = packageManager.getLaunchIntentForPackage(packageName) ?: return
        launch.putExtra(OPEN_NOTIFICATIONS_EXTRA, true)
        val pending = PendingIntent.getActivity(
            this,
            message.data["notification_id"]?.hashCode() ?: 0,
            launch,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val category = message.data["category"]?.takeIf { it.isNotBlank() } ?: "system"
        val severity = message.data["severity"]?.takeIf { it.isNotBlank() } ?: "info"
        val notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_jarvis)
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(
                NotificationCompat.BigTextStyle().bigText(
                    "$body\n${notificationCategoryLabel(category)} · ${notificationSeverityLabel(severity)}",
                ),
            )
            .setContentIntent(pending)
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .build()
        val id = message.data["notification_id"]?.hashCode() ?: System.currentTimeMillis().toInt()
        NotificationManagerCompat.from(this).notify(id, notification)
    }

    companion object {
        const val CHANNEL_ID = "jarvis_notifications"
        const val OPEN_NOTIFICATIONS_EXTRA = "local.jarvis.app.OPEN_NOTIFICATIONS"
        private const val TAG = "JarvisPush"
    }
}
