package local.jarvis.app

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import local.jarvis.client.CoreClient
import local.jarvis.client.DeviceSession

/**
 * Notifiche in tempo reale (ADR-0098). Mentre l'app è in primo piano il Core avvisa subito quando ne arriva una:
 * la schermata si aggiorna senza attendere il polling, che resta come rete di sicurezza e rallenta mentre il
 * flusso è attivo. In background restano le notifiche push.
 */
class LiveSignal internal constructor(
    private val events: Channel<Unit>,
    private val connected: () -> Boolean,
) {
    /** Attende il prossimo segnale del Core oppure, in mancanza, il tempo normale di polling. */
    suspend fun pause(normalMs: Long) {
        val wait = if (connected()) maxOf(normalMs, 60_000L) else normalMs
        withTimeoutOrNull(wait) { events.receive() }
    }
}

@Composable
fun rememberLiveSignal(client: CoreClient, session: DeviceSession, foreground: Boolean): LiveSignal {
    val events = remember(session) { Channel<Unit>(Channel.CONFLATED) }
    var connected by remember(session) { mutableStateOf(false) }
    LaunchedEffect(session, foreground) {
        if (!foreground) {
            connected = false
            return@LaunchedEffect
        }
        while (isActive) {
            withContext(Dispatchers.IO) {
                runCatching {
                    client.watchNotices(
                        session,
                        keepGoing = { isActive },
                        onReady = { connected = true },
                        onChange = { events.trySend(Unit) },
                    )
                }
            }
            connected = false
            delay(5_000)
        }
    }
    return remember(events) { LiveSignal(events) { connected } }
}
