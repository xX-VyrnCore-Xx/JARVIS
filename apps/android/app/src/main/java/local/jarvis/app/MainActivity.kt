package local.jarvis.app

import android.Manifest
import android.os.Build
import android.os.Bundle
import android.content.pm.PackageManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.compose.setContent
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.Canvas
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import local.jarvis.client.ChatMessage
import local.jarvis.client.CoreClient
import local.jarvis.client.CoreException
import local.jarvis.client.DeviceSession
import local.jarvis.client.Notices
import local.jarvis.client.PendingAction
import local.jarvis.client.TaskSummary

/** Client Android di JARVIS (ADR-0018, ADR-0032): associazione, chat, conferme, attività e notifiche. */
class MainActivity : ComponentActivity() {
    private val appInForeground = mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val store = SessionStore(applicationContext)
        setContent {
            JarvisTheme(dark = isSystemInDarkTheme()) {
                var session by remember { mutableStateOf(store.load()) }
                Scaffold { padding ->
                    Column(Modifier.padding(padding).fillMaxSize().imePadding()) {
                        val current = session
                        if (current == null) {
                            PairingScreen { paired -> store.save(paired); session = paired }
                        } else {
                            DeviceScreen(current, appInForeground.value) { store.clear(); session = null }
                        }
                    }
                }
            }
        }
    }

    override fun onStart() {
        super.onStart()
        appInForeground.value = true
    }

    override fun onStop() {
        appInForeground.value = false
        super.onStop()
    }
}

private val client = CoreClient()

@Composable
private fun PairingScreen(onPaired: (DeviceSession) -> Unit) {
    var origin by remember { mutableStateOf("https://") }
    var code by remember { mutableStateOf("") }
    var name by remember { mutableStateOf(Build.MODEL ?: "Telefono Android") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    Column(Modifier.fillMaxSize().imePadding().padding(horizontal = 24.dp, vertical = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(22.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("JARVIS", style = MaterialTheme.typography.headlineLarge, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
                Text("Il tuo assistente, sul tuo dispositivo", style = MaterialTheme.typography.titleMedium)
                Text("Connessione HTTPS protetta", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("Associa questo dispositivo", style = MaterialTheme.typography.titleLarge)
            Text("Sul computer apri Dispositivi, genera un codice monouso e inseriscilo qui. Scade dopo 10 minuti.")
            OutlinedTextField(origin, { origin = it }, label = { Text("Indirizzo del Core (HTTPS)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(code, { code = it.uppercase() }, label = { Text("Codice di associazione") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(name, { name = it }, label = { Text("Nome del dispositivo") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        }
        Button(enabled = !busy, onClick = {
            busy = true
            error = null
            scope.launch {
                val result = withContext(Dispatchers.IO) { runCatching { client.pair(origin, code, name) } }
                busy = false
                result.onSuccess(onPaired).onFailure { error = it.message ?: "Associazione non riuscita" }
            }
        }, modifier = Modifier.fillMaxWidth()) { Text(if (busy) "Associazione…" else "Associa dispositivo") }
    }
}

/** Schede del dispositivo: chat, attività e notifiche (le rotte permesse dal permesso `chat`). */
@Composable
private fun ColumnScope.DeviceScreen(session: DeviceSession, appInForeground: Boolean, onRevoked: () -> Unit) {
    var tab by rememberSaveable { mutableStateOf(0) }
    var sending by remember { mutableStateOf(false) }
    val tabs = listOf("Plancia" to "⌂", "Chat" to "◉", "Attività" to "≡", "Notifiche" to "◇", "Core" to "◎")
    Text(
        tabs[tab].first,
        modifier = Modifier.padding(horizontal = 22.dp, vertical = 12.dp),
        style = MaterialTheme.typography.headlineLarge,
        fontWeight = FontWeight.Bold,
    )
    AnimatedContent(
        targetState = tab,
        modifier = Modifier.weight(1f).fillMaxWidth(),
        transitionSpec = {
            val direction = if (targetState > initialState) 1 else -1
            fadeIn(tween(200)) + slideInHorizontally(tween(200)) { direction * it / 12 } togetherWith fadeOut(tween(110))
        },
        label = "mobile-tab",
    ) { selected ->
        Column(Modifier.fillMaxSize()) {
            when (selected) {
                0 -> HomeScreen(
                    session = session,
                    appInForeground = appInForeground,
                    onRevoked = onRevoked,
                    onOpenChat = { tab = 1 },
                    onOpenTasks = { tab = 2 },
                )
                1 -> ChatScreen(session, appInForeground, onRevoked) { sending = it }
                2 -> TasksScreen(session, appInForeground, onRevoked)
                3 -> NoticesScreen(session, appInForeground, onRevoked)
                else -> CoreScreen(session, onRevoked)
            }
        }
    }
    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
    NavigationBar(containerColor = MaterialTheme.colorScheme.surface) {
        tabs.forEachIndexed { index, (label, symbol) ->
            NavigationBarItem(
                selected = tab == index,
                onClick = { tab = index },
                enabled = !sending,
                icon = { Text(symbol, style = MaterialTheme.typography.titleLarge) },
                label = { Text(label) },
            )
        }
    }
}

private data class HomeSnapshot(
    val recentTask: TaskSummary?,
    val recentTaskCount: Int,
    val unreadCount: Int,
)

@Composable
private fun ColumnScope.HomeScreen(
    session: DeviceSession,
    appInForeground: Boolean,
    onRevoked: () -> Unit,
    onOpenChat: () -> Unit,
    onOpenTasks: () -> Unit,
) {
    var snapshot by remember { mutableStateOf<HomeSnapshot?>(null) }
    var status by remember { mutableStateOf<String?>(null) }
    var loading by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    suspend fun refresh() {
        loading = true
        val result = withContext(Dispatchers.IO) {
            runCatching {
                val tasks = client.tasks(session)
                val notices = client.notices(session)
                HomeSnapshot(tasks.firstOrNull(), tasks.size, notices.unread)
            }
        }
        result
            .onSuccess {
                snapshot = it
                status = null
            }
            .onFailure {
                if (it is CoreException.Revoked) onRevoked()
                else status = it.message ?: "Impossibile aggiornare la plancia."
            }
        loading = false
    }

    LaunchedEffect(session, appInForeground) {
        if (!appInForeground) return@LaunchedEffect
        while (isActive) {
            refresh()
            delay(15_000)
        }
    }

    Column(
        Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Card(
            Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Column(
                Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 14.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                AvatarPortrait()
                Text(
                    "JARVIS",
                    style = MaterialTheme.typography.headlineMedium,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.primary,
                )
                Text(
                    "Il tuo assistente personale",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(12.dp))
                Surface(
                    color = MaterialTheme.colorScheme.primaryContainer,
                    shape = MaterialTheme.shapes.small,
                ) {
                    Text(
                        if (loading && snapshot == null) "Connessione al Core…"
                        else if (status != null) "Connessione non verificata"
                        else if (snapshot != null) "●  Core collegato"
                        else "In attesa della connessione",
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onPrimaryContainer,
                    )
                }
            }
        }

        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Button(onClick = onOpenChat, modifier = Modifier.weight(1f)) {
                Text("Parla con JARVIS")
            }
            OutlinedButton(onClick = onOpenTasks, modifier = Modifier.weight(1f)) {
                Text("Attività")
            }
        }

        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            HomeMetric(
                label = "TASK RECENTI",
                value = snapshot?.recentTaskCount?.toString() ?: "—",
                modifier = Modifier.weight(1f),
            )
            HomeMetric(
                label = "DA LEGGERE",
                value = snapshot?.unreadCount?.toString() ?: "—",
                modifier = Modifier.weight(1f),
            )
        }

        Card(Modifier.fillMaxWidth()) {
            Column(
                Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Ultima attività", style = MaterialTheme.typography.titleMedium)
                    TextButton(
                        enabled = !loading,
                        onClick = { scope.launch { refresh() } },
                    ) {
                        Text(if (loading) "Aggiorno…" else "Aggiorna")
                    }
                }
                val task = snapshot?.recentTask
                if (task == null) {
                    Text(
                        if (snapshot == null) "Le attività appariranno qui quando il Core sarà raggiungibile."
                        else "Nessuna attività recente.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                } else {
                    Text(task.objective, style = MaterialTheme.typography.bodyLarge)
                    Text(
                        "${STATE_LABEL[task.state] ?: task.state} · ${task.createdAt.take(16).replace('T', ' ')}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }

        status?.let {
            Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun HomeMetric(label: String, value: String, modifier: Modifier = Modifier) {
    Card(
        modifier,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
    ) {
        Column(
            Modifier.fillMaxWidth().padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(label, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(value, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
        }
    }
}

@Composable
private fun AvatarPortrait() {
    val transition = rememberInfiniteTransition(label = "jarvis-avatar")
    val phase by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(1900), repeatMode = RepeatMode.Reverse),
        label = "avatar-breath",
    )
    val accent = MaterialTheme.colorScheme.primary
    val face = MaterialTheme.colorScheme.onSurfaceVariant
    Canvas(Modifier.fillMaxWidth().height(188.dp)) {
        val center = Offset(size.width / 2f, size.height / 2f)
        val unit = size.minDimension / 188.dp.toPx()
        val pulse = phase * 3.dp.toPx()
        drawCircle(
            accent.copy(alpha = 0.10f + phase * 0.08f),
            radius = size.minDimension * 0.43f + pulse,
            center = center,
            style = Stroke(width = 1.5.dp.toPx()),
        )
        drawCircle(
            accent.copy(alpha = 0.32f),
            radius = size.minDimension * 0.35f,
            center = center,
            style = Stroke(width = 1.dp.toPx()),
        )
        drawArc(
            accent.copy(alpha = 0.78f),
            startAngle = -90f + phase * 18f,
            sweepAngle = 220f,
            useCenter = false,
            topLeft = Offset(center.x - size.minDimension * 0.39f, center.y - size.minDimension * 0.39f),
            size = Size(size.minDimension * 0.78f, size.minDimension * 0.78f),
            style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round),
        )

        val head = Path().apply {
            moveTo(center.x, center.y - 42.dp.toPx())
            cubicTo(center.x - 26.dp.toPx(), center.y - 42.dp.toPx(), center.x - 33.dp.toPx(), center.y - 22.dp.toPx(), center.x - 31.dp.toPx(), center.y + 2.dp.toPx())
            cubicTo(center.x - 28.dp.toPx(), center.y + 27.dp.toPx(), center.x - 13.dp.toPx(), center.y + 35.dp.toPx(), center.x, center.y + 38.dp.toPx())
            cubicTo(center.x + 13.dp.toPx(), center.y + 35.dp.toPx(), center.x + 28.dp.toPx(), center.y + 27.dp.toPx(), center.x + 31.dp.toPx(), center.y + 2.dp.toPx())
            cubicTo(center.x + 33.dp.toPx(), center.y - 22.dp.toPx(), center.x + 26.dp.toPx(), center.y - 42.dp.toPx(), center.x, center.y - 42.dp.toPx())
            close()
        }
        drawPath(head, Color(0xFF142D40))
        drawPath(head, accent.copy(alpha = 0.9f), style = Stroke(width = 1.7.dp.toPx(), cap = StrokeCap.Round))

        val shoulders = Path().apply {
            moveTo(center.x - 55.dp.toPx(), center.y + 73.dp.toPx())
            quadraticTo(center.x - 48.dp.toPx(), center.y + 37.dp.toPx(), center.x - 17.dp.toPx(), center.y + 31.dp.toPx())
            lineTo(center.x, center.y + 40.dp.toPx())
            lineTo(center.x + 17.dp.toPx(), center.y + 31.dp.toPx())
            quadraticTo(center.x + 48.dp.toPx(), center.y + 37.dp.toPx(), center.x + 55.dp.toPx(), center.y + 73.dp.toPx())
            close()
        }
        drawPath(shoulders, Color(0xFF10283A))
        drawPath(shoulders, accent.copy(alpha = 0.70f), style = Stroke(width = 1.6.dp.toPx()))
        drawLine(accent, Offset(center.x - 19.dp.toPx(), center.y - 7.dp.toPx()), Offset(center.x - 5.dp.toPx(), center.y - 7.dp.toPx()), 2.4.dp.toPx(), StrokeCap.Round)
        drawLine(accent, Offset(center.x + 5.dp.toPx(), center.y - 7.dp.toPx()), Offset(center.x + 19.dp.toPx(), center.y - 7.dp.toPx()), 2.4.dp.toPx(), StrokeCap.Round)
        drawLine(face, Offset(center.x, center.y - 1.dp.toPx()), Offset(center.x - 4.dp.toPx(), center.y + 12.dp.toPx()), 1.dp.toPx(), StrokeCap.Round)
        drawLine(face, Offset(center.x - 10.dp.toPx(), center.y + 21.dp.toPx()), Offset(center.x + 10.dp.toPx(), center.y + 21.dp.toPx()), 1.dp.toPx(), StrokeCap.Round)

        for (index in 0 until 11) {
            val x = center.x - 25.dp.toPx() + index * 5.dp.toPx()
            val height = (5f + ((index * 7 + phase * 5).toInt() % 9)) * unit
            drawLine(
                accent.copy(alpha = 0.45f + phase * 0.35f),
                Offset(x, center.y + 63.dp.toPx() - height / 2),
                Offset(x, center.y + 63.dp.toPx() + height / 2),
                1.5.dp.toPx(),
                StrokeCap.Round,
            )
        }
    }
}

@Composable
private fun CoreScreen(session: DeviceSession, onRevoked: () -> Unit) {
    var disconnect by rememberSaveable { mutableStateOf(false) }
    var status by remember { mutableStateOf("Verifica in corso…") }
    var checking by remember { mutableStateOf(true) }
    val scope = rememberCoroutineScope()
    suspend fun check() {
        checking = true
        val result = withContext(Dispatchers.IO) { runCatching { client.syncChat(session) } }
        result.onSuccess { status = "Core raggiungibile · sessione attiva" }.onFailure {
            if (it is CoreException.Revoked) onRevoked()
            else status = it.message ?: "Core non raggiungibile"
        }
        checking = false
    }
    LaunchedEffect(session) { check() }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Connessione HTTPS", style = MaterialTheme.typography.titleMedium)
                Text(session.origin)
                Text(status, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(if (session.canConfirm) "Accesso a chat e conferme" else "Accesso alla chat e alle attività")
                TextButton(enabled = !checking, onClick = { scope.launch { check() } }) { Text("Verifica connessione") }
            }
        }
        OutlinedButton(onClick = { disconnect = true }) { Text("Rimuovi sessione dal telefono") }
    }
    if (disconnect) {
        AlertDialog(
            onDismissRequest = { disconnect = false },
            title = { Text("Rimuovere la sessione?") },
            text = { Text("Per ricollegarti servirà un nuovo codice. Dal computer puoi revocare l’accesso al dispositivo.") },
            confirmButton = { TextButton(onClick = onRevoked) { Text("Rimuovi") } },
            dismissButton = { TextButton(onClick = { disconnect = false }) { Text("Annulla") } },
        )
    }
}

private val STATE_LABEL = mapOf(
    "succeeded" to "riuscita", "failed" to "fallita", "waiting_confirmation" to "attende conferma",
    "running" to "in corso", "cancelled" to "annullata", "planned" to "pianificata", "pending" to "in attesa",
)

@Composable
private fun ColumnScope.TasksScreen(session: DeviceSession, appInForeground: Boolean, onRevoked: () -> Unit) {
    val tasks = remember { mutableStateListOf<TaskSummary>() }
    var status by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    suspend fun refresh() {
        withContext(Dispatchers.IO) { runCatching { client.tasks(session) } }
            .onSuccess { tasks.clear(); tasks.addAll(it); status = if (it.isEmpty()) "Nessuna attività." else null }
            .onFailure { if (it is CoreException.Revoked) onRevoked() else status = it.message }
    }
    LaunchedEffect(session, appInForeground) {
        if (!appInForeground) return@LaunchedEffect
        while (isActive) {
            refresh()
            delay(5_000)
        }
    }
    TextButton(onClick = { scope.launch { refresh() } }, modifier = Modifier.padding(horizontal = 8.dp)) { Text("Aggiorna") }
    status?.let { Text(it, modifier = Modifier.padding(horizontal = 16.dp)) }
    LazyColumn(Modifier.weight(1f).fillMaxWidth().padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        items(tasks) { t ->
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(12.dp)) {
                    Text(t.objective, style = MaterialTheme.typography.bodyLarge)
                    Text("${STATE_LABEL[t.state] ?: t.state} · ${t.createdAt.take(16).replace('T', ' ')}", style = MaterialTheme.typography.bodySmall,
                        color = if (t.state == "failed") MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

@Composable
private fun ColumnScope.NoticesScreen(session: DeviceSession, appInForeground: Boolean, onRevoked: () -> Unit) {
    var notices by remember { mutableStateOf<Notices?>(null) }
    var status by remember { mutableStateOf<String?>(null) }
    val context = LocalContext.current
    val store = remember { SessionStore(context) }
    var pushEnabled by remember { mutableStateOf(store.pushEnabled()) }
    var pushStatus by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    fun registerToken() {
        if (!JarvisApplication.firebaseConfigured()) {
            pushStatus = "Push non configurato: completa i parametri Firebase per Android."
            return
        }
        val firebase = FirebaseApp.getApps(context).firstOrNull { it.name == FirebaseApp.DEFAULT_APP_NAME }
        if (firebase == null) {
            pushStatus = "Firebase non inizializzato. Verifica la configurazione della build."
            return
        }
        FirebaseMessaging.getInstance().token.addOnCompleteListener { task ->
            if (!task.isSuccessful) {
                pushStatus = "Impossibile ottenere il token push. Controlla la connessione e riprova."
                return@addOnCompleteListener
            }
            scope.launch {
                withContext(Dispatchers.IO) { runCatching { client.registerPush(task.result, session) } }
                    .onSuccess { pushStatus = "Notifiche in background attive su questo dispositivo." }
                    .onFailure {
                        if (it is CoreException.Revoked) onRevoked()
                        else pushStatus = it.message ?: "Registrazione push non riuscita."
                    }
            }
        }
    }

    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (granted) {
            store.setPushEnabled(true)
            pushEnabled = true
            registerToken()
        } else {
            store.setPushEnabled(false)
            pushEnabled = false
            pushStatus = "Permesso notifiche negato nelle impostazioni di Android."
        }
    }

    fun enablePush() {
        if (!JarvisApplication.firebaseConfigured()) {
            pushStatus = "Push non configurato: completa i parametri Firebase per Android."
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            permissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else if (!NotificationManagerCompat.from(context).areNotificationsEnabled()) {
            pushStatus = "Le notifiche di JARVIS sono disattivate nelle impostazioni di Android."
        } else {
            store.setPushEnabled(true)
            pushEnabled = true
            registerToken()
        }
    }

    fun disablePush() {
        store.setPushEnabled(false)
        pushEnabled = false
        val firebase = FirebaseApp.getApps(context).firstOrNull { it.name == FirebaseApp.DEFAULT_APP_NAME }
        val unregister = {
            scope.launch {
                withContext(Dispatchers.IO) { runCatching { client.removePush(session) } }
                    .onSuccess { pushStatus = "Notifiche in background disattivate." }
                    .onFailure {
                        if (it is CoreException.Revoked) onRevoked()
                        else pushStatus = it.message ?: "Rimozione della registrazione push non riuscita."
                    }
            }
        }
        if (firebase == null) unregister()
        else FirebaseMessaging.getInstance().deleteToken().addOnCompleteListener { unregister() }
    }

    suspend fun refresh() {
        withContext(Dispatchers.IO) { runCatching { client.notices(session) } }
            .onSuccess { notices = it; status = if (it.items.isEmpty()) "Nessuna notifica." else null }
            .onFailure { if (it is CoreException.Revoked) onRevoked() else status = it.message }
    }
    LaunchedEffect(session, appInForeground) {
        if (!appInForeground) return@LaunchedEffect
        if (pushEnabled && JarvisApplication.firebaseConfigured() &&
            (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
                ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED)
        ) registerToken()
        while (isActive) {
            refresh()
            delay(5_000)
        }
    }
    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Notifiche in background", style = MaterialTheme.typography.titleSmall)
            Text(
                if (pushEnabled) "JARVIS può avvisarti anche quando non stai usando l'app."
                else "Attiva il push per ricevere gli avvisi senza tenere aperta questa schermata.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (pushEnabled) {
                OutlinedButton(onClick = ::disablePush) { Text("Disattiva") }
            } else {
                Button(onClick = ::enablePush) { Text("Attiva notifiche push") }
            }
            pushStatus?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
    HorizontalDivider(Modifier.padding(horizontal = 16.dp))
    Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), horizontalArrangement = Arrangement.SpaceBetween) {
        Text("Non lette: ${notices?.unread ?: 0}", modifier = Modifier.padding(8.dp))
        Row {
            TextButton(onClick = { scope.launch { refresh() } }) { Text("Aggiorna") }
            TextButton(onClick = {
                scope.launch {
                    withContext(Dispatchers.IO) { runCatching { client.markAllRead(session) } }
                        .onFailure { if (it is CoreException.Revoked) onRevoked() else status = it.message }
                    refresh()
                }
            }) { Text("Segna lette") }
        }
    }
    status?.let { Text(it, modifier = Modifier.padding(horizontal = 16.dp)) }
    LazyColumn(Modifier.weight(1f).fillMaxWidth().padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        items(notices?.items ?: emptyList()) { n ->
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(12.dp)) {
                    Text((if (n.read) "" else "● ") + n.title, style = MaterialTheme.typography.titleSmall,
                        color = when (n.severity) { "error" -> MaterialTheme.colorScheme.error; else -> MaterialTheme.colorScheme.onSurface })
                    if (n.body.isNotEmpty()) Text(n.body, style = MaterialTheme.typography.bodySmall)
                    Text(n.createdAt.take(16).replace('T', ' '), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

@Composable
private fun ColumnScope.ChatScreen(session: DeviceSession, appInForeground: Boolean, onRevoked: () -> Unit, onSending: (Boolean) -> Unit) {
    val messages = remember { mutableStateListOf<ChatMessage>() }
    val pending = remember { mutableStateListOf<PendingAction>() }
    var input by rememberSaveable { mutableStateOf("") }
    var status by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val list = rememberLazyListState()

    fun handle(e: Throwable) {
        if (e is CoreException.Revoked) onRevoked() else status = e.message ?: "Errore di rete"
    }

    suspend fun refresh() {
        val result = withContext(Dispatchers.IO) { runCatching { client.syncChat(session) } }
        result.onSuccess { (history, actions) ->
            val stickToEnd = messages.isEmpty() || !list.canScrollForward
            val historyChanged = messages != history
            val actionsChanged = pending != actions
            if (historyChanged) { messages.clear(); messages.addAll(history) }
            if (actionsChanged) { pending.clear(); pending.addAll(actions) }
            val lastItem = messages.size + pending.size - 1
            if (stickToEnd && lastItem >= 0 && (historyChanged || actionsChanged)) {
                list.scrollToItem(lastItem)
            }
            status = null
        }.onFailure(::handle)
    }

    LaunchedEffect(session, appInForeground) {
        if (!appInForeground) return@LaunchedEffect
        while (isActive) {
            if (!busy) refresh()
            delay(4_000)
        }
    }

    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.SpaceBetween) {
        Text("Conversazione", style = MaterialTheme.typography.titleMedium, modifier = Modifier.align(Alignment.CenterVertically))
        TextButton(onClick = { scope.launch { refresh() } }) { Text("Aggiorna") }
    }
    status?.let { Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(horizontal = 16.dp)) }
    LazyColumn(Modifier.weight(1f).fillMaxWidth().padding(horizontal = 16.dp), state = list, contentPadding = PaddingValues(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        items(messages) { m ->
            val fromUser = m.role == "user"
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = if (fromUser) Arrangement.End else Arrangement.Start,
            ) {
                Card(
                    Modifier.fillMaxWidth(0.9f),
                    colors = CardDefaults.cardColors(
                        containerColor = if (fromUser) MaterialTheme.colorScheme.primaryContainer
                        else MaterialTheme.colorScheme.surfaceVariant,
                        contentColor = if (fromUser) MaterialTheme.colorScheme.onPrimaryContainer
                        else MaterialTheme.colorScheme.onSurfaceVariant,
                    ),
                ) {
                    Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(
                            if (fromUser) "Tu" else "JARVIS",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.primary,
                            fontWeight = FontWeight.SemiBold,
                        )
                        Text(m.content, style = MaterialTheme.typography.bodyLarge)
                    }
                }
            }
        }
        items(pending, key = { "confirmation-${it.confirmationId}" }) { action ->
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Conferma richiesta", style = MaterialTheme.typography.titleSmall)
                    Text(action.expectedEffect)
                    Text(action.target, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                    if (session.canConfirm) {
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            for ((label, approve) in listOf("Approva" to true, "Rifiuta" to false)) {
                                val decide = {
                                    scope.launch {
                                        withContext(Dispatchers.IO) { runCatching { client.decide(action, approve, session) } }.onFailure(::handle)
                                        refresh()
                                    }
                                }
                                if (approve) Button(onClick = { decide() }) { Text(label) } else OutlinedButton(onClick = { decide() }) { Text(label) }
                            }
                        }
                    } else {
                        Text("Si conferma dal computer: questo dispositivo non ha il permesso di confermare.", style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        }
    }
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedTextField(
                input, { input = it }, enabled = !busy, maxLines = 4,
                placeholder = { Text("Scrivi a JARVIS…") },
                shape = MaterialTheme.shapes.large,
                modifier = Modifier.weight(1f),
            )
            Button(enabled = !busy && input.isNotBlank(), onClick = {
                val text = input.trim()
                busy = true
                onSending(true)
                scope.launch {
                    val result = withContext(Dispatchers.IO) { runCatching { client.send(text, session) } }
                    result.onSuccess {
                        input = ""
                        refresh()
                    }.onFailure(::handle)
                    busy = false
                    onSending(false)
                }
            }) { Text(if (busy) "Invio…" else "Invia") }
        }
    }
}
