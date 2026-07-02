package fm.bae.app.ui

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.StringRes
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
import fm.bae.app.BaeLogger
import fm.bae.app.OAuthLinker
import fm.bae.app.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.JoinFromCodeOperation
import uniffi.bae_bridge.RestoreFromCodeOperation
import uniffi.bae_bridge.decodeRestoreCode
import uniffi.bae_bridge.generateJoinRequest
import uniffi.bae_bridge.joinFromCodeOperation
import uniffi.bae_bridge.restoreFromCodeOperation

private const val TAG = "bae.OnboardingScreen"
private val logger = BaeLogger(TAG)

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
                resolveOauthToken(oauthLinking, oauthLinkingError, context, info.cloudProvider)
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
 * One in-progress join attempt: decode the invite code, run OAuth if the
 * library's provider needs it, then join from the code on a background thread.
 * Mirrors [LinkFlow] exactly — the only difference is which bridge code it
 * decodes (an invite code, not a restore code) and that it joins as a new
 * member rather than restoring this device's own library. The blocking
 * `join()` bridge call can't be interrupted by coroutine cancellation, so
 * [cancel] also cancels the operation's own token.
 */
private class JoinFlow(
    val job: Job,
) {
    var joinOperation: JoinFromCodeOperation? = null

    suspend fun execute(
        code: String,
        oauthTokenJson: String?,
        onJoined: (BridgeLibrary) -> Unit,
    ) {
        // OAuth (and the account-email fetch) already ran when the provider was
        // picked; the token captured then is reused here — no second sign-in.
        val operation = joinFromCodeOperation(code = code, oauthTokenJson = oauthTokenJson)
        joinOperation = operation
        val libraryInfo = withContext(Dispatchers.IO) { operation.join() }
        onJoined(libraryInfo)
    }

    fun cancel() {
        joinOperation?.cancel()
        job.cancel()
    }
}

/** The OAuth providers, for which the joiner authenticates before generating its code. */
private fun providerUsesOauth(provider: BridgeCloudProvider): Boolean =
    when (provider) {
        BridgeCloudProvider.GOOGLE_DRIVE,
        BridgeCloudProvider.DROPBOX,
        BridgeCloudProvider.ONE_DRIVE,
        -> true

        BridgeCloudProvider.S3,
        BridgeCloudProvider.CLOUD_KIT,
        -> false
    }

private suspend fun resolveOauthToken(
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    context: Context,
    provider: BridgeCloudProvider,
): String {
    val oauthError =
        oauthLinkingError
            ?: if (oauthLinking == null) context.getString(R.string.onboarding_oauth_unconfigured) else null
    if (oauthError != null) throw IllegalStateException(oauthError)
    return oauthLinking!!.authorize(context, provider)
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
                    logger.debug("link flow cancelled by bridge", e)
                } catch (e: CancellationException) {
                    logger.debug("link flow coroutine cancelled", e)
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

/**
 * The join twin of [LinkLauncher], plus the up-front provider step joining needs.
 * The joiner first picks the target library's provider ([selectProvider]); for an
 * OAuth provider that authenticates and fetches the account email, then generates
 * this device's join-request with the email baked in. The captured token is held
 * and reused by [join] — no second sign-in. Same attempt/supersede/cleanup
 * discipline as [LinkLauncher] for the join itself.
 */
class JoinLauncher(
    private val scope: CoroutineScope,
    private val context: Context,
    private val onJoined: (BridgeLibrary) -> Unit,
) {
    var error by mutableStateOf<String?>(null)
    private var flow by mutableStateOf<JoinFlow?>(null)
    val isJoining: Boolean get() = flow != null

    // Provider-prep state, read by JoinLibraryScreen.
    var provider by mutableStateOf<BridgeCloudProvider?>(null)
        private set
    var requestCode by mutableStateOf<String?>(null)
        private set
    var fingerprint by mutableStateOf<String?>(null)
        private set
    var generateError by mutableStateOf<String?>(null)
        private set
    var isAuthorizing by mutableStateOf(false)
        private set

    // The token/email captured at provider selection: the token is reused for
    // the join, the email is baked into the generated code.
    private var oauthTokenJson: String? = null
    private var prepJob: Job? = null

    /**
     * Pick the provider the target library uses and prepare this device's
     * join-request. For an OAuth provider this authenticates and fetches the
     * account email up front; for S3/iCloud it generates the code with no email.
     */
    fun selectProvider(
        provider: BridgeCloudProvider,
        oauthLinking: OAuthLinker?,
        oauthLinkingError: String?,
    ) {
        prepJob?.cancel()
        error = null
        generateError = null
        oauthTokenJson = null
        requestCode = null
        fingerprint = null
        this.provider = provider
        prepJob =
            scope.launch {
                try {
                    var email: String? = null
                    if (providerUsesOauth(provider)) {
                        isAuthorizing = true
                        val oauthError =
                            oauthLinkingError
                                ?: if (oauthLinking == null) {
                                    context.getString(R.string.onboarding_oauth_unconfigured)
                                } else {
                                    null
                                }
                        if (oauthError != null) {
                            generateError = oauthError
                            isAuthorizing = false
                            return@launch
                        }
                        val token = oauthLinking!!.authorize(context, provider)
                        email = oauthLinking.fetchAccountEmail(provider, token)
                        oauthTokenJson = token
                        isAuthorizing = false
                    }
                    val request = withContext(Dispatchers.IO) { generateJoinRequest(email) }
                    requestCode = request.code
                    fingerprint = request.fingerprint
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    logger.error("Failed to prepare join request", e)
                    isAuthorizing = false
                    generateError = e.message ?: context.getString(R.string.onboarding_join_generate_failed)
                }
            }
    }

    /** Return to the provider picker, dropping the generated code and token. */
    fun resetProvider() {
        prepJob?.cancel()
        provider = null
        requestCode = null
        fingerprint = null
        generateError = null
        isAuthorizing = false
        oauthTokenJson = null
        error = null
    }

    fun join(code: String) {
        // Reuse the token captured at provider selection — no second OAuth.
        val oauthTokenJson = this.oauthTokenJson
        error = null
        flow?.cancel()
        lateinit var started: JoinFlow
        val launched =
            scope.launch(start = CoroutineStart.LAZY) {
                try {
                    started.execute(code, oauthTokenJson, onJoined)
                } catch (e: BridgeException.Cancelled) {
                    logger.debug("join flow cancelled by bridge", e)
                } catch (e: CancellationException) {
                    logger.debug("join flow coroutine cancelled", e)
                    throw e
                } catch (e: Exception) {
                    error = e.toString()
                } finally {
                    if (flow === started) flow = null
                }
            }
        started = JoinFlow(launched)
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
    val onJoinLibrary: () -> Unit,
)

/** Which onboarding code the QR scanner is currently capturing. */
private enum class ScanTarget {
    RESTORE_CODE,
    INVITE_CODE,
}

/**
 * Returns a callback that opens the scanner for a [ScanTarget], requesting camera
 * permission first when it isn't already held. Requesting a scan clears the
 * target's prior error via [setError]; [onOpen] runs once the camera is available
 * (synchronously when permission is held, or after the grant); [setError] receives
 * the camera-permission-required message, tagged with its target, on denial.
 */
@Composable
private fun rememberScanRequest(
    setError: (ScanTarget, String?) -> Unit,
    onOpen: (ScanTarget) -> Unit,
): (ScanTarget) -> Unit {
    val context = LocalContext.current
    // The target a pending camera-permission request is for; read in the grant
    // callback to know which scanner to open and which target an error is for.
    var pendingScanTarget by remember { mutableStateOf<ScanTarget?>(null) }
    val permissionLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            val target = pendingScanTarget
            pendingScanTarget = null
            if (target != null) {
                if (granted) {
                    onOpen(target)
                } else {
                    setError(target, context.getString(R.string.onboarding_camera_permission_required))
                }
            } else {
                logger.warning("camera permission result arrived with no pending scan target")
            }
        }
    val hasCameraPermission =
        ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
            PackageManager.PERMISSION_GRANTED
    return { target ->
        setError(target, null)
        if (hasCameraPermission) {
            onOpen(target)
        } else {
            pendingScanTarget = target
            permissionLauncher.launch(Manifest.permission.CAMERA)
        }
    }
}

@Composable
private fun OnboardingScanner(
    target: ScanTarget,
    onScanned: (ScanTarget, String) -> Unit,
    onDismiss: () -> Unit,
) {
    val instructions =
        when (target) {
            ScanTarget.RESTORE_CODE -> stringResource(R.string.qr_scanner_instructions)
            ScanTarget.INVITE_CODE -> stringResource(R.string.qr_scanner_invite_instructions)
        }
    QRScannerScreen(
        onScanned = { code -> onScanned(target, code) },
        onDismiss = onDismiss,
        instructions = instructions,
    )
}

@Composable
fun OnboardingScreen(
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    onLinked: (BridgeLibrary) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val launcher = remember { LinkLauncher(scope, context, onLinked) }
    val joinLauncher = remember { JoinLauncher(scope, context, onLinked) }
    var showJoin by remember { mutableStateOf(false) }
    // Non-null while the scanner is open, identifying which code it captures.
    var scanTarget by remember { mutableStateOf<ScanTarget?>(null) }

    val onRequestScan =
        rememberScanRequest(
            setError = { target, message ->
                if (target == ScanTarget.INVITE_CODE) joinLauncher.error = message else launcher.error = message
            },
            onOpen = { scanTarget = it },
        )

    when {
        scanTarget != null -> {
            OnboardingScanner(
                target = scanTarget!!,
                onScanned = { target, code ->
                    scanTarget = null
                    when (target) {
                        ScanTarget.RESTORE_CODE -> launcher.link(code, oauthLinking, oauthLinkingError)
                        ScanTarget.INVITE_CODE -> joinLauncher.join(code)
                    }
                },
                onDismiss = { scanTarget = null },
            )
        }

        launcher.isLinking -> {
            OnboardingProgress(linking = true) { launcher.cancel() }
        }

        joinLauncher.isJoining -> {
            OnboardingProgress(linking = false) { joinLauncher.cancel() }
        }

        showJoin -> {
            JoinLibraryScreen(
                joinLauncher = joinLauncher,
                oauthLinking = oauthLinking,
                oauthLinkingError = oauthLinkingError,
                onRequestScan = { onRequestScan(ScanTarget.INVITE_CODE) },
                onBack = {
                    joinLauncher.resetProvider()
                    showJoin = false
                },
            )
        }

        else -> {
            OnboardingIdleScreen(
                launcher = launcher,
                oauthLinking = oauthLinking,
                oauthLinkingError = oauthLinkingError,
                onRequestScan = { onRequestScan(ScanTarget.RESTORE_CODE) },
                onJoinLibrary = { showJoin = true },
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
    onJoinLibrary: () -> Unit,
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
                onJoinLibrary = onJoinLibrary,
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
        Button(onClick = callbacks.onJoinLibrary, modifier = buttonWidth) {
            Text(stringResource(R.string.onboarding_join_library))
        }
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedButton(onClick = callbacks.onScanQR, modifier = buttonWidth) {
            Text(stringResource(R.string.onboarding_scan_qr))
        }
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedButton(onClick = callbacks.onShowPasteDialog, modifier = buttonWidth) {
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
            text =
                PasteDialogText(
                    title = stringResource(R.string.onboarding_paste_code),
                    instructions = stringResource(R.string.onboarding_paste_code_instructions),
                    placeholder = stringResource(R.string.onboarding_restore_code_placeholder),
                    confirmLabel = stringResource(R.string.onboarding_connect),
                ),
            pasteInput = pasteInput,
            onInputChange = callbacks.onPasteInputChange,
            onConfirm = callbacks.onConnect,
            onDismiss = callbacks.onDismissPaste,
        )
    }
}

/** The fixed wording of a [PasteCodeDialog], distinct per code it accepts. */
class PasteDialogText(
    val title: String,
    val instructions: String,
    val placeholder: String,
    val confirmLabel: String,
)

@Composable
fun PasteCodeDialog(
    text: PasteDialogText,
    pasteInput: String,
    onInputChange: (String) -> Unit,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(text.title) },
        text = {
            Column {
                Text(
                    text = text.instructions,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(modifier = Modifier.height(12.dp))
                OutlinedTextField(
                    value = pasteInput,
                    onValueChange = onInputChange,
                    placeholder = { Text(text.placeholder) },
                    modifier = Modifier.fillMaxWidth(),
                    textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
                    singleLine = false,
                    maxLines = 3,
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onConfirm(pasteInput.trim()) }, enabled = pasteInput.trim().isNotEmpty()) {
                Text(text.confirmLabel)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/**
 * The waiting screen for a running attempt: connecting to restore this device's
 * own library when [linking], or joining an existing library otherwise.
 */
@Composable
private fun OnboardingProgress(
    linking: Boolean,
    onCancel: () -> Unit,
) {
    if (linking) {
        ProgressScreen(
            R.string.onboarding_connecting_title,
            R.string.onboarding_connecting_body,
            onCancel,
        )
    } else {
        ProgressScreen(
            R.string.onboarding_joining_title,
            R.string.onboarding_joining_body,
            onCancel,
        )
    }
}

@Composable
private fun ProgressScreen(
    @StringRes titleRes: Int,
    @StringRes bodyRes: Int,
    onCancel: () -> Unit,
) {
    OnboardingContainer {
        CircularProgressIndicator()
        Spacer(modifier = Modifier.height(24.dp))
        Text(
            text = stringResource(titleRes),
            style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(8.dp))
        Text(
            text = stringResource(bodyRes),
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
