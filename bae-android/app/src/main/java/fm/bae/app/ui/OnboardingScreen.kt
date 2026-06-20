package fm.bae.app.ui

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.util.Log
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import fm.bae.app.OAuthLinker
import fm.bae.app.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.RestoreFromCodeOperation
import uniffi.bae_bridge.decodeRestoreCode
import uniffi.bae_bridge.restoreFromCodeOperation

private const val TAG = "bae.OnboardingScreen"

private class LinkFlow(
    val job: Job,
) {
    var restoreOperation: RestoreFromCodeOperation? = null

    suspend fun execute(
        code: String,
        oauthLinking: OAuthLinker?,
        oauthLinkingError: String?,
        context: Context,
        onLinked: (BridgeLibrary) -> Unit,
    ) {
        val info = decodeRestoreCode(code)
        val oauthTokenJson =
            if (info.needsOauth) {
                val oauthError =
                    oauthLinkingError
                        ?: if (oauthLinking == null) context.getString(R.string.onboarding_oauth_unconfigured) else null
                if (oauthError != null) throw IllegalStateException(oauthError)
                oauthLinking!!.authorize(context, info.cloudProvider)
            } else {
                null
            }
        val operation = restoreFromCodeOperation(code = code, oauthTokenJson = oauthTokenJson)
        restoreOperation = operation
        val libraryInfo = withContext(Dispatchers.IO) { operation.restore() }
        onLinked(libraryInfo)
    }

    fun cancel() {
        restoreOperation?.cancel()
        job.cancel()
    }
}

/**
 * Holds the in-progress link attempt and its UI state (error text, whether a
 * link is running). Created once per onboarding via [remember] so its snapshot
 * state survives recomposition; the screen reads [error]/[isLinking] and drives
 * [link]/[cancel]. A new attempt cancels any prior one; the finally block only
 * clears state if it still owns the current attempt (identity check), so a
 * superseded attempt's completion can't wipe a newer one's state.
 */
private class LinkLauncher(
    private val scope: CoroutineScope,
    private val context: Context,
    private val onLinked: (BridgeLibrary) -> Unit,
) {
    var error by mutableStateOf<String?>(null)
    private var flow by mutableStateOf<LinkFlow?>(null)
    val isLinking: Boolean get() = flow != null

    fun link(
        code: String,
        oauthLinking: OAuthLinker?,
        oauthLinkingError: String?,
    ) {
        error = null
        flow?.cancel()
        lateinit var started: LinkFlow
        val launched =
            scope.launch(start = CoroutineStart.LAZY) {
                try {
                    started.execute(code, oauthLinking, oauthLinkingError, context, onLinked)
                } catch (e: BridgeException.Cancelled) {
                    Log.d(TAG, "link flow cancelled by bridge", e)
                } catch (e: CancellationException) {
                    Log.d(TAG, "link flow coroutine cancelled", e)
                    throw e
                } catch (e: Exception) {
                    error = e.toString()
                } finally {
                    if (flow === started) flow = null
                }
            }
        started = LinkFlow(launched)
        flow = started
        launched.start()
    }

    fun cancel() {
        flow?.cancel()
        flow = null
    }
}

private class OnboardingIdleCallbacks(
    val onScanQR: () -> Unit,
    val onShowPasteDialog: () -> Unit,
    val onPasteInputChange: (String) -> Unit,
    val onConnect: (String) -> Unit,
    val onDismissPaste: () -> Unit,
)

@Composable
fun OnboardingScreen(
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    onLinked: (BridgeLibrary) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val launcher = remember { LinkLauncher(scope, context, onLinked) }
    var showScanner by remember { mutableStateOf(false) }

    val cameraPermissionLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) {
                showScanner = true
            } else {
                launcher.error = context.getString(R.string.onboarding_camera_permission_required)
            }
        }
    val hasCameraPermission =
        ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
            PackageManager.PERMISSION_GRANTED
    val onRequestScan = {
        launcher.error = null
        if (hasCameraPermission) {
            showScanner = true
        } else {
            cameraPermissionLauncher.launch(Manifest.permission.CAMERA)
        }
    }

    when {
        showScanner -> {
            QRScannerScreen(
                onScanned = { code ->
                    showScanner = false
                    launcher.link(code, oauthLinking, oauthLinkingError)
                },
                onDismiss = { showScanner = false },
            )
        }

        launcher.isLinking -> {
            LinkingScreen(onCancel = { launcher.cancel() })
        }

        else -> {
            OnboardingIdleScreen(
                launcher = launcher,
                oauthLinking = oauthLinking,
                oauthLinkingError = oauthLinkingError,
                onRequestScan = onRequestScan,
            )
        }
    }
}

@Composable
private fun OnboardingIdleScreen(
    launcher: LinkLauncher,
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    onRequestScan: () -> Unit,
) {
    var showPasteDialog by remember { mutableStateOf(false) }
    var pasteInput by remember { mutableStateOf("") }
    OnboardingIdleContent(
        error = launcher.error,
        showPasteDialog = showPasteDialog,
        pasteInput = pasteInput,
        callbacks =
            OnboardingIdleCallbacks(
                onScanQR = onRequestScan,
                onShowPasteDialog = {
                    launcher.error = null
                    pasteInput = ""
                    showPasteDialog = true
                },
                onPasteInputChange = { pasteInput = it },
                onConnect = { code ->
                    showPasteDialog = false
                    launcher.link(code, oauthLinking, oauthLinkingError)
                },
                onDismissPaste = { showPasteDialog = false },
            ),
    )
}

@Composable
private fun OnboardingIdleContent(
    error: String?,
    showPasteDialog: Boolean,
    pasteInput: String,
    callbacks: OnboardingIdleCallbacks,
) {
    OnboardingContainer {
        Spacer(modifier = Modifier.weight(1f))
        Image(
            painter = painterResource(R.drawable.sheep_icon),
            contentDescription = stringResource(R.string.onboarding_icon_description),
            modifier = Modifier.size(120.dp),
        )
        Spacer(modifier = Modifier.height(8.dp))
        Text(text = "bae", fontSize = 48.sp, fontWeight = FontWeight.Bold)
        Spacer(modifier = Modifier.height(12.dp))
        Text(
            text = stringResource(R.string.onboarding_tagline),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(32.dp))
        val buttonWidth = Modifier.width(200.dp)
        Button(onClick = callbacks.onScanQR, modifier = buttonWidth) {
            Text(stringResource(R.string.onboarding_scan_qr))
        }
        Spacer(modifier = Modifier.height(8.dp))
        Button(onClick = callbacks.onShowPasteDialog, modifier = buttonWidth) {
            Text(stringResource(R.string.onboarding_paste_code))
        }
        if (error != null) {
            Spacer(modifier = Modifier.height(8.dp))
            Text(text = error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
        }
        Spacer(modifier = Modifier.weight(1f))
    }
    if (showPasteDialog) {
        PasteCodeDialog(
            pasteInput = pasteInput,
            onInputChange = callbacks.onPasteInputChange,
            onConnect = callbacks.onConnect,
            onDismiss = callbacks.onDismissPaste,
        )
    }
}

@Composable
private fun PasteCodeDialog(
    pasteInput: String,
    onInputChange: (String) -> Unit,
    onConnect: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.onboarding_paste_code)) },
        text = {
            Column {
                Text(
                    text = stringResource(R.string.onboarding_paste_code_instructions),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(modifier = Modifier.height(12.dp))
                OutlinedTextField(
                    value = pasteInput,
                    onValueChange = onInputChange,
                    placeholder = { Text(stringResource(R.string.onboarding_restore_code_placeholder)) },
                    modifier = Modifier.fillMaxWidth(),
                    textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
                    singleLine = false,
                    maxLines = 3,
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onConnect(pasteInput.trim()) }, enabled = pasteInput.trim().isNotEmpty()) {
                Text(stringResource(R.string.onboarding_connect))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

@Composable
private fun LinkingScreen(onCancel: () -> Unit) {
    OnboardingContainer {
        CircularProgressIndicator()
        Spacer(modifier = Modifier.height(24.dp))
        Text(
            text = stringResource(R.string.onboarding_connecting_title),
            style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(8.dp))
        Text(
            text = stringResource(R.string.onboarding_connecting_body),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(24.dp))
        OutlinedButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
    }
}

@Composable
private fun OnboardingContainer(content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier.fillMaxSize().padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
        content = content,
    )
}
