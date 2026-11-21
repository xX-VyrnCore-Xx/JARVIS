package local.jarvis.app

import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
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
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val store = SessionStore(applicationContext)
        setContent {
            MaterialTheme(colorScheme = if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme()) {
                var session by remember { mutableStateOf(store.load()) }
                Scaffold { padding ->
                    Column(Modifier.padding(padding).fillMaxSize().imePadding()) {
                        val current = session
                        if (current == null) {
                            PairingScreen { paired -> store.save(paired); session = paired }
                        } else {
                            DeviceScreen(current) { store.clear(); session = null }
                        }
                    }
                }
            }
        }
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
    Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("JARVIS", style = MaterialTheme.typography.headlineMedium)
        Text("Sul computer apri la sezione Dispositivi, genera un codice e inseriscilo qui. Vale una volta sola per 10 minuti.")
        OutlinedTextField(origin, { origin = it }, label = { Text("Indirizzo del Core (HTTPS)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(code, { code = it.uppercase() }, label = { Text("Codice di associazione") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(name, { name = it }, label = { Text("Nome del dispositivo") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        Button(enabled = !busy, onClick = {
            busy = true
            error = null
            scope.launch {
                val result = withContext(Dispatchers.IO) { runCatching { client.pair(origin, code, name) } }
                busy = false
                result.onSuccess(onPaired).onFailure { error = it.message ?: "Associazione non riuscita" }
            }
        }) { Text(if (busy) "Associazione…" else "Associa") }
    }
}

/** Schede del dispositivo: chat, attività e notifiche (le rotte permesse dal permesso `chat`). */
@Composable
private fun ColumnScope.DeviceScreen(session: DeviceSession, onRevoked: () -> Unit) {
    var tab by remember { mutableStateOf(0) }
    TabRow(selectedTabIndex = tab) {
        listOf("Chat", "Attività", "Notifiche").forEachIndexed { i, label ->
            Tab(selected = tab == i, onClick = { tab = i }, text = { Text(label) })
        }
    }
    when (tab) {
        0 -> ChatScreen(session, onRevoked)
        1 -> TasksScreen(session, onRevoked)
        else -> NoticesScreen(session, onRevoked)
    }
}

private val STATE_LABEL = mapOf(
    "succeeded" to "riuscita", "failed" to "fallita", "waiting_confirmation" to "attende conferma",
    "running" to "in corso", "cancelled" to "annullata", "planned" to "pianificata", "pending" to "in attesa",
)

@Composable
private fun ColumnScope.TasksScreen(session: DeviceSession, onRevoked: () -> Unit) {
    val tasks = remember { mutableStateListOf<TaskSummary>() }
    var status by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    suspend fun refresh() {
        withContext(Dispatchers.IO) { runCatching { client.tasks(session) } }
            .onSuccess { tasks.clear(); tasks.addAll(it); status = if (it.isEmpty()) "Nessuna attività." else null }
            .onFailure { if (it is CoreException.Revoked) onRevoked() else status = it.message }
    }
    LaunchedEffect(session) {
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
private fun ColumnScope.NoticesScreen(session: DeviceSession, onRevoked: () -> Unit) {
    var notices by remember { mutableStateOf<Notices?>(null) }
    var status by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    suspend fun refresh() {
        withContext(Dispatchers.IO) { runCatching { client.notices(session) } }
            .onSuccess { notices = it; status = if (it.items.isEmpty()) "Nessuna notifica." else null }
            .onFailure { if (it is CoreException.Revoked) onRevoked() else status = it.message }
    }
    LaunchedEffect(session) {
        while (isActive) {
            refresh()
            delay(5_000)
        }
    }
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
private fun ColumnScope.ChatScreen(session: DeviceSession, onRevoked: () -> Unit) {
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

    LaunchedEffect(session) {
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
            Card(Modifier.fillMaxWidth()) {
                Text((if (m.role == "user") "Tu: " else "JARVIS: ") + m.content, Modifier.padding(12.dp))
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
