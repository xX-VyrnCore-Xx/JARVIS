package local.jarvis.app

import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import local.jarvis.client.AppRelease
import local.jarvis.client.AppUpdate
import java.io.File

/** Aggiornamento dell'app dall'interno (ADR-0112): controlla, scarica, verifica e consegna l'APK al sistema. */
private object AppInstaller {
    fun dir(context: Context) = File(context.cacheDir, "updates")

    fun currentVersion(context: Context): String =
        context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: "0.0.0"

    @Suppress("DEPRECATION")
    fun verifyPackage(context: Context, apk: File) {
        val manager = context.packageManager
        val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P)
            PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
        val archive = manager.getPackageArchiveInfo(apk.absolutePath, flags)
        check(archive?.packageName == context.packageName) { "pacchetto Android non corrispondente" }
        val installed = manager.getPackageInfo(context.packageName, flags)
        fun PackageInfo.signers() = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P)
            signingInfo?.apkContentsSigners.orEmpty().toSet() else signatures.orEmpty().toSet()
        val trusted = installed.signers()
        check(trusted.isNotEmpty() && archive?.signers() == trusted) { "firma APK non corrispondente" }
    }

    fun canInstall(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.O || context.packageManager.canRequestPackageInstalls()

    fun installPermissionIntent(context: Context) =
        Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}"))

    fun install(context: Context, apk: File) {
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.updates", apk)
        context.startActivity(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }
}

private sealed interface UpdateState {
    data object Idle : UpdateState
    data object Checking : UpdateState
    data object UpToDate : UpdateState
    data class Available(val release: AppRelease) : UpdateState
    data class Downloading(val release: AppRelease, val done: Long, val total: Long) : UpdateState
    data class Ready(val release: AppRelease, val file: File) : UpdateState
    data class Failed(val message: String) : UpdateState
}

@Composable
fun UpdateCard(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var state by remember { mutableStateOf<UpdateState>(UpdateState.Idle) }

    val permission = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        val ready = state as? UpdateState.Ready
        if (ready != null && AppInstaller.canInstall(context)) {
            runCatching { AppInstaller.install(context, ready.file) }
                .onFailure { state = UpdateState.Failed("Installazione non disponibile: ${it.message}") }
        }
    }

    fun check() {
        state = UpdateState.Checking
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { AppUpdate.check(AppInstaller.currentVersion(context)) }
            }
            state = result.fold(
                onSuccess = { if (it == null) UpdateState.UpToDate else UpdateState.Available(it) },
                onFailure = { UpdateState.Failed("Controllo non riuscito: ${it.message ?: "rete non disponibile"}") },
            )
        }
    }

    fun download(release: AppRelease) {
        state = UpdateState.Downloading(release, 0, release.apkSize)
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val apk = AppUpdate.download(release, AppInstaller.dir(context)) { done, total ->
                        state = UpdateState.Downloading(release, done, total)
                    }
                    try {
                        AppInstaller.verifyPackage(context, apk)
                        apk.parentFile?.listFiles()?.filter { it != apk && it.name.endsWith("_android-release.apk") }
                            ?.forEach { it.delete() }
                        apk
                    } catch (error: Exception) {
                        apk.delete()
                        throw error
                    }
                }
            }
            state = result.fold(
                onSuccess = { UpdateState.Ready(release, it) },
                onFailure = { UpdateState.Failed("Download non riuscito: ${it.message ?: "interrotto"}") },
            )
        }
    }

    Card(
        modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Aggiornamenti", style = MaterialTheme.typography.titleMedium)
            when (val current = state) {
                UpdateState.Idle -> {
                    Text(
                        "Versione installata ${AppInstaller.currentVersion(context)}.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    OutlinedButton(onClick = { check() }) { Text("Controlla aggiornamenti") }
                }
                UpdateState.Checking -> Text("Controllo in corso…", style = MaterialTheme.typography.bodyMedium)
                UpdateState.UpToDate -> {
                    Text("JARVIS è aggiornato.", style = MaterialTheme.typography.bodyMedium)
                    OutlinedButton(onClick = { check() }) { Text("Ricontrolla") }
                }
                is UpdateState.Available -> {
                    Text(
                        "Nuova versione ${current.release.version} disponibile.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Button(onClick = { download(current.release) }) { Text("Scarica aggiornamento") }
                }
                is UpdateState.Downloading -> {
                    val fraction = if (current.total > 0) (current.done.toFloat() / current.total).coerceIn(0f, 1f) else 0f
                    Text(
                        "Scarico ${current.release.version}: ${current.done / 1_000_000} / ${current.total / 1_000_000} MB",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    LinearProgressIndicator(progress = { fraction }, modifier = Modifier.fillMaxWidth())
                }
                is UpdateState.Ready -> {
                    Text(
                        "JARVIS ${current.release.version} è scaricato e verificato (SHA-256 e firma APK).",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Text(
                        "Android chiederà di confermare l'installazione; i tuoi dati restano nell'app.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(onClick = {
                            runCatching {
                                if (AppInstaller.canInstall(context)) AppInstaller.install(context, current.file)
                                else permission.launch(AppInstaller.installPermissionIntent(context))
                            }.onFailure { state = UpdateState.Failed("Installazione non disponibile: ${it.message}") }
                        }) { Text(if (AppInstaller.canInstall(context)) "Installa" else "Consenti e installa") }
                        OutlinedButton(onClick = { state = UpdateState.Idle }) { Text("Più tardi") }
                    }
                }
                is UpdateState.Failed -> {
                    Text(current.message, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                    OutlinedButton(onClick = { check() }) { Text("Riprova") }
                }
            }
        }
    }
}
