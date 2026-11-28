package local.jarvis.app

import android.Manifest
import android.os.Build
import android.os.Bundle
import android.content.Intent
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
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
import local.jarvis.client.PairingLink
import local.jarvis.client.PendingAction
import local.jarvis.client.TaskSummary

private const val NOTIFICATIONS_TAB_INDEX = 3

internal fun notificationCategoryLabel(category: String): String = when (category) {
    "confirmation" -> "Conferma"
    "integration" -> "Integrazione"
    "reminder" -> "Promemoria"
    "briefing" -> "Briefing"
    "mission" -> "Missione"
    "suggestion" -> "Suggerimento"
    "security" -> "Sicurezza"
    "system" -> "Sistema"
    "automation" -> "Automazione"
    else -> category.replaceFirstChar { if (it.isLowerCase()) it.titlecase() else it.toString() }
}

internal fun notificationSeverityLabel(severity: String): String = when (severity) {
    "info" -> "Informazione"
    "success" -> "Completata"
    "warning" -> "Attenzione"
    "error" -> "Errore"
    else -> severity.replaceFirstChar { if (it.isLowerCase()) it.titlecase() else it.toString() }
}

/** Client Android di JARVIS (ADR-0018, ADR-0032): associazione, chat, conferme, attività e notifiche. */
class MainActivity : ComponentActivity() {
    private val appInForeground = mutableStateOf(false)
    private val openNotificationId = mutableStateOf<String?>(null)
    /** Collegamento di associazione aperto da fuori (messaggio, AirDrop, browser). */
    private val pendingLink = mutableStateOf<PairingLink?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pendingLink.value = PairingLink.parse(intent?.dataString)
        openNotificationId.value = intent?.getStringExtra(PushMessagingService.NOTIFICATION_ID_EXTRA)
        val store = SessionStore(applicationContext)
        setContent {
            JarvisTheme(dark = isSystemInDarkTheme()) {
                var session by remember { mutableStateOf(store.load()) }
                Scaffold { padding ->
                    Column(Modifier.padding(padding).fillMaxSize().imePadding()) {
                        val current = session
                        if (current == null) {
                            PairingScreen(initialLink = pendingLink.value) { paired -> store.save(paired); session = paired }
                        } else {
                            DeviceScreen(
                                session = current,
                                appInForeground = appInForeground.value,
                                initialTab = if (intent?.getBooleanExtra(
                                        PushMessagingService.OPEN_NOTIFICATIONS_EXTRA,
                                        false,
                                    ) == true
                                    || openNotificationId.value != null
                                ) NOTIFICATIONS_TAB_INDEX else 0,
                                notificationId = openNotificationId.value,
                                onRevoked = { store.clear(); session = null },
                            )
                        }
                    }
                }
            }
        }
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        openNotificationId.value =
            intent.getStringExtra(PushMessagingService.NOTIFICATION_ID_EXTRA)
        PairingLink.parse(intent.dataString)?.let { pendingLink.value = it }
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
private fun PairingScreen(initialLink: PairingLink?, onPaired: (DeviceSession) -> Unit) {
    var origin by remember { mutableStateOf("https://") }
    var code by remember { mutableStateOf("") }
    var name by remember { mutableStateOf(Build.MODEL ?: "Telefono Android") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var checking by remember { mutableStateOf(false) }
    var verified by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    // Un collegamento aperto da fuori compila indirizzo e codice: resta solo da toccare «Associa».
    LaunchedEffect(initialLink) {
        initialLink?.let {
            origin = it.origin
            code = it.code
            verified = false
            error = null
        }
    }
    Column(Modifier.fillMaxSize().imePadding().padding(horizontal = 24.dp, vertical = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(22.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("JARVIS", style = MaterialTheme.typography.headlineLarge, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
                Text("Il tuo assistente, sul tuo dispositivo", style = MaterialTheme.typography.titleMedium)
                Text("Associazione tramite HTTPS", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("Associa questo dispositivo", style = MaterialTheme.typography.titleLarge)
            Text("Sul computer apri Dispositivi, tocca «Condividi collegamento» e aprilo su questo telefono: indirizzo e codice si compilano da soli. Il codice scade dopo 10 minuti.")
            Button(enabled = !busy && !checking, onClick = {
                val link = PairingLink.parse(clipboard.getText()?.text)
                if (link == null) {
                    error = "Negli appunti non c'è un collegamento di associazione di JARVIS."
                } else {
                    origin = link.origin
                    code = link.code
                    verified = false
                    error = null
                }
            }, modifier = Modifier.fillMaxWidth()) { Text("Incolla il collegamento") }
            OutlinedTextField(origin, {
                origin = it
                verified = false
                error = null
            }, enabled = !busy && !checking, label = { Text("Indirizzo del Core (HTTPS)") },
                singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedButton(enabled = !busy && !checking, onClick = {
                checking = true
                error = null
                verified = false
                val target = origin
                scope.launch {
                    val result = withContext(Dispatchers.IO) { runCatching { client.checkConnectivity(target) } }
                    checking = false
                    result.onSuccess { verified = true }.onFailure { error = it.message ?: "Core non raggiungibile" }
                }
            }) { Text(if (checking) "Verifica…" else "Verifica HTTPS") }
            if (verified) {
                Text("Certificato valido · Core raggiungibile", color = MaterialTheme.colorScheme.primary)
            }
            OutlinedTextField(code, { code = it.uppercase() }, label = { Text("Codice di associazione") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(name, { name = it }, label = { Text("Nome del dispositivo") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        }
        Button(enabled = !busy && !checking && code.isNotBlank() && name.isNotBlank(), onClick = {
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
private fun ColumnScope.DeviceScreen(
    session: DeviceSession,
    appInForeground: Boolean,
    initialTab: Int,
    notificationId: String?,
    onRevoked: () -> Unit,
) {
    var tab by rememberSaveable { mutableStateOf(initialTab) }
    var linkedTaskId by rememberSaveable { mutableStateOf<String?>(null) }
    var chatDraft by rememberSaveable { mutableStateOf<String?>(null) }
    var sending by remember { mutableStateOf(false) }
    val tabs = listOf("Plancia", "Chat", "Attività", "Notifiche", "Core")
    var unread by remember { mutableStateOf(0) }
    LaunchedEffect(session, appInForeground, tab) {
        if (!appInForeground) return@LaunchedEffect
        while (isActive) {
            val result = withContext(Dispatchers.IO) { runCatching { client.notices(session).unread } }
            result.onSuccess { unread = it }.onFailure { if (it is CoreException.Revoked) onRevoked() }
            delay(20_000)
        }
    }
    LaunchedEffect(notificationId) {
        if (notificationId != null) tab = NOTIFICATIONS_TAB_INDEX
    }
    Surface(color = MaterialTheme.colorScheme.surfaceContainer) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 22.dp, vertical = 12.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(tabs[tab], style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            if (tab == NOTIFICATIONS_TAB_INDEX && unread > 0) {
                Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.secondaryContainer) {
                    Text("$unread da leggere", Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                        style = MaterialTheme.typography.labelMedium)
                }
            }
        }
    }
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
                    onResumeTask = { objective ->
                        chatDraft = "Riprendi l'attività: $objective"
                        tab = 1
                    },
                )
                1 -> ChatScreen(
                    session = session,
                    appInForeground = appInForeground,
                    onRevoked = onRevoked,
                    initialDraft = chatDraft,
                    onDraftConsumed = { chatDraft = null },
                    onSending = { sending = it },
                )
                2 -> TasksScreen(
                    session,
                    appInForeground,
                    onRevoked,
                    linkedTaskId,
                    onResumeTask = { objective ->
                        chatDraft = "Riprendi l'attività: $objective"
                        tab = 1
                    },
                )
                3 -> NoticesScreen(
                    session = session,
                    appInForeground = appInForeground,
                    onRevoked = onRevoked,
                    focusNotificationId = notificationId,
                    onOpenLink = { link ->
                        when (link.view) {
                            "chat" -> tab = 1
                            "tasks" -> {
                                linkedTaskId = link.ref
                                tab = 2
                            }
                            "dashboard" -> tab = 0
                        }
                    },
                )
                else -> CoreScreen(session, onRevoked)
            }
        }
    }
    HorizontalDivider(thickness = 0.5.dp, color = MaterialTheme.colorScheme.outlineVariant)
    JarvisTabBar(
        labels = tabs,
        selected = tab,
        enabled = !sending,
        badges = mapOf(NOTIFICATIONS_TAB_INDEX to unread),
        onSelect = { tab = it },
    )
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
    onResumeTask: (String) -> Unit,
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

    val live = rememberLiveSignal(client, session, appInForeground)
    LaunchedEffect(session, appInForeground) {
        if (!appInForeground) return@LaunchedEffect
        while (isActive) {
            refresh()
            live.pause(15_000)
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
                JarvisAvatar(Modifier.fillMaxWidth().height(248.dp), thinking = loading)
                Text(
                    "JARVIS",
                    style = MaterialTheme.typography.headlineMedium,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.primary,
                )
                Text(
                    when {
                        snapshot == null -> "Il tuo assistente personale"
                        (snapshot?.unreadCount ?: 0) == 1 -> "Una notifica da leggere"
                        (snapshot?.unreadCount ?: 0) > 1 -> "${snapshot?.unreadCount} notifiche da leggere"
                        else -> "Tutto in ordine"
                    },
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

        UpdateCard()

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
                    TextButton(onClick = { onResumeTask(task.objective) }) {
                        Text("Prepara la ripresa in chat")
                    }
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
private fun ColumnScope.TasksScreen(
    session: DeviceSession,
    appInForeground: Boolean,
    onRevoked: () -> Unit,
    selectedTaskId: String?,
    onResumeTask: (String) -> Unit,
) {
    val tasks = remember { mutableStateListOf<TaskSummary>() }
    var status by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val listState = rememberLazyListState()
    suspend fun refresh() {
        withContext(Dispatchers.IO) { runCatching { client.tasks(session) } }
            .onSuccess { tasks.clear(); tasks.addAll(it); status = if (it.isEmpty()) "Nessuna attività." else null }
            .onFailure { if (it is CoreException.Revoked) onRevoked() else status = it.message }
    }
    val live = rememberLiveSignal(client, session, appInForeground)
    LaunchedEffect(session, appInForeground) {
        if (!appInForeground) return@LaunchedEffect
        while (isActive) {
            refresh()
            live.pause(5_000)
        }
    }
    LaunchedEffect(tasks.toList(), selectedTaskId) {
        val index = tasks.indexOfFirst { it.id == selectedTaskId }
        if (index >= 0) listState.animateScrollToItem(index)
    }
    TextButton(onClick = { scope.launch { refresh() } }, modifier = Modifier.padding(horizontal = 8.dp)) { Text("Aggiorna") }
    status?.let { Text(it, modifier = Modifier.padding(horizontal = 16.dp)) }
    if (selectedTaskId != null && tasks.isNotEmpty() && tasks.none { it.id == selectedTaskId }) {
        Text("L'attività collegata non è tra le più recenti.", modifier = Modifier.padding(horizontal = 16.dp), style = MaterialTheme.typography.bodySmall)
    }
    LazyColumn(
        Modifier.weight(1f).fillMaxWidth().padding(horizontal = 16.dp),
        state = listState,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(tasks) { t ->
            Card(
                Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(
                    containerColor = if (t.id == selectedTaskId) MaterialTheme.colorScheme.primaryContainer
                    else MaterialTheme.colorScheme.surface,
                ),
            ) {
                Column(Modifier.padding(12.dp)) {
                    Text(t.objective, style = MaterialTheme.typography.bodyLarge)
                    Text("${STATE_LABEL[t.state] ?: t.state} · ${t.createdAt.take(16).replace('T', ' ')}", style = MaterialTheme.typography.bodySmall,
                        color = if (t.state == "failed") MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant)
                    TextButton(onClick = { onResumeTask(t.objective) }) {
                        Text("Prepara la ripresa in chat")
                    }
                }
            }
        }
    }
}

@Composable
private fun ColumnScope.NoticesScreen(
    session: DeviceSession,
    appInForeground: Boolean,
    onRevoked: () -> Unit,
    focusNotificationId: String?,
    onOpenLink: (local.jarvis.client.NotificationLink) -> Unit,
) {
    var notices by remember { mutableStateOf<Notices?>(null) }
    var status by remember { mutableStateOf<String?>(null) }
    val context = LocalContext.current
    val store = remember { SessionStore(context) }
    var pushEnabled by remember { mutableStateOf(store.pushEnabled()) }
    var pushStatus by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val listState = rememberLazyListState()

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

    val live = rememberLiveSignal(client, session, appInForeground)
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
            live.pause(5_000)
        }
    }
    LaunchedEffect(notices, focusNotificationId) {
        val index = notices?.items?.indexOfFirst { it.id == focusNotificationId } ?: -1
        if (index >= 0) listState.animateScrollToItem(index)
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
    LazyColumn(
        Modifier.weight(1f).fillMaxWidth().padding(horizontal = 16.dp),
        state = listState,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(notices?.items ?: emptyList()) { n ->
            Card(
                Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(
                    containerColor = if (n.id == focusNotificationId) MaterialTheme.colorScheme.primaryContainer
                    else MaterialTheme.colorScheme.surface,
                ),
            ) {
                Column(
                    Modifier.padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    Row(
                        Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            n.title,
                            style = MaterialTheme.typography.titleSmall,
                            fontWeight = if (n.read) FontWeight.Normal else FontWeight.SemiBold,
                            color = when (n.severity) {
                                "error" -> MaterialTheme.colorScheme.error
                                "warning" -> MaterialTheme.colorScheme.tertiary
                                else -> MaterialTheme.colorScheme.onSurface
                            },
                            modifier = Modifier.weight(1f),
                        )
                        if (!n.read) {
                            Surface(
                                color = MaterialTheme.colorScheme.primaryContainer,
                                shape = MaterialTheme.shapes.small,
                            ) {
                                Text(
                                    "Nuova",
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onPrimaryContainer,
                                )
                            }
                        }
                    }
                    Text(
                        "${notificationCategoryLabel(n.category.ifBlank { "system" })} · ${notificationSeverityLabel(n.severity)}",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    if (n.body.isNotEmpty()) Text(n.body, style = MaterialTheme.typography.bodyMedium)
                    n.link?.let { link ->
                        val destination = when (link.view) {
                            "chat" -> "Apri chat"
                            "tasks" -> "Apri attività"
                            "dashboard" -> "Apri plancia"
                            else -> null
                        }
                        if (destination != null) {
                            TextButton(onClick = { onOpenLink(link) }) { Text(destination) }
                        } else {
                            Text(
                                "Questa vista non è supportata nell'app Android.",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                    Row(
                        Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            n.createdAt.take(16).replace('T', ' '),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        if (!n.read) {
                            TextButton(onClick = {
                                scope.launch {
                                    withContext(Dispatchers.IO) {
                                        runCatching { client.markRead(n.id, session) }
                                    }.onFailure {
                                        if (it is CoreException.Revoked) onRevoked()
                                        else status = it.message
                                    }
                                    refresh()
                                }
                            }) {
                                Text("Segna come letta")
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ColumnScope.ChatScreen(
    session: DeviceSession,
    appInForeground: Boolean,
    onRevoked: () -> Unit,
    initialDraft: String?,
    onDraftConsumed: () -> Unit,
    onSending: (Boolean) -> Unit,
) {
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

    LaunchedEffect(initialDraft) {
        if (initialDraft != null) {
            input = initialDraft
            onDraftConsumed()
        }
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
                // Fumetti come in iMessage: blu di sistema per te, grigio per JARVIS, angolo «a coda» sul lato di chi scrive.
                Surface(
                    modifier = Modifier.widthIn(max = 320.dp),
                    shape = RoundedCornerShape(
                        topStart = 20.dp,
                        topEnd = 20.dp,
                        bottomStart = if (fromUser) 20.dp else 6.dp,
                        bottomEnd = if (fromUser) 6.dp else 20.dp,
                    ),
                    color = if (fromUser) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surface,
                    contentColor = if (fromUser) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
                ) {
                    Column(
                        Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
                        verticalArrangement = Arrangement.spacedBy(2.dp),
                    ) {
                        if (!fromUser) {
                            Text(
                                "JARVIS",
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
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
                shape = MaterialTheme.shapes.extraLarge,
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
