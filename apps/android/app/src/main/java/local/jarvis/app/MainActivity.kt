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
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
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
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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
    var tab by remember { mutableStateOf(0) }
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Column {
            Text("JARVIS", style = MaterialTheme.typography.titleLarge, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
            Text("Dispositivo associato · HTTPS", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Text("●", color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.titleMedium)
    }
    TabRow(selectedTabIndex = tab) {
        listOf("Chat", "Attività", "Notifiche").forEachIndexed { i, label ->
            Tab(selected = tab == i, onClick = { tab = i }, text = { Text(label) })
        }
    }
    AnimatedContent(
        targetState = tab,
        modifier = Modifier.weight(1f).fillMaxWidth(),
        transitionSpec = { fadeIn(tween(160)) + slideInVertically { it / 12 } togetherWith fadeOut(tween(100)) },
        label = "mobile-tab",
    ) { selected ->
        Column(Modifier.fillMaxSize()) {
            when (selected) {
                0 -> ChatScreen(session, appInForeground, onRevoked)
                1 -> TasksScreen(session, appInForeground, onRevoked)
                else -> NoticesScreen(session, appInForeground, onRevoked)
            }
        }
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
private fun ColumnScope.ChatScreen(session: DeviceSession, appInForeground: Boolean, onRevoked: () -> Unit) {
    val messages = remember { mutableStateListOf<ChatMessage>() }
    val pending = remember { mutableStateListOf<PendingAction>() }
    var input by remember { mutableStateOf("") }
    var status by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val list = rememberLazyListState()

    fun handle(e: Throwable) {
        if (e is CoreException.Revoked) onRevoked() else status = e.message ?: "Errore di rete"
    }

    suspend fun refresh() {
        val result = withContext(Dispatchers.IO) { runCatching { client.history(session) to client.pending(session) } }
        result.onSuccess { (history, actions) ->
            val stickToEnd = messages.isEmpty() || !list.canScrollForward
            if (messages != history) {
                messages.clear(); messages.addAll(history)
                if (stickToEnd && messages.isNotEmpty()) list.scrollToItem(messages.size - 1)
            }
            if (pending != actions) { pending.clear(); pending.addAll(actions) }
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
        Text("JARVIS", style = MaterialTheme.typography.titleLarge)
        TextButton(onClick = { scope.launch { refresh() } }) { Text("Aggiorna") }
    }
    status?.let { Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(horizontal = 16.dp)) }
    for (action in pending) {
        Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
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
    LazyColumn(Modifier.weight(1f).fillMaxWidth().padding(horizontal = 16.dp), state = list, verticalArrangement = Arrangement.spacedBy(8.dp)) {
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
    }
    Row(Modifier.fillMaxWidth().padding(16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(input, { input = it }, placeholder = { Text("Scrivi a JARVIS…") }, modifier = Modifier.weight(1f))
        Button(enabled = !busy && input.isNotBlank(), onClick = {
            val text = input.trim()
            input = ""
            busy = true
            messages.add(ChatMessage("user", text))
            scope.launch {
                withContext(Dispatchers.IO) { runCatching { client.send(text, session) } }.onFailure(::handle)
                busy = false
                refresh()
            }
        }) { Text("Invia") }
    }
}
