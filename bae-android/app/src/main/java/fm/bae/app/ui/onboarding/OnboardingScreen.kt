package fm.bae.app.ui.onboarding

import android.Manifest
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
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import fm.bae.app.BaeLogger
import fm.bae.app.OAuthLinker
import fm.bae.app.R
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.components.QRScannerScreen
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeLibrary

private const val TAG = "bae.OnboardingScreen"
private val logger = BaeLogger(TAG)

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
    PAIRING_CODE,
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
            ScanTarget.PAIRING_CODE -> stringResource(R.string.onboarding_join_pairing_instructions)
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
                if (target == ScanTarget.PAIRING_CODE) joinLauncher.error = message else launcher.error = message
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

                        // Fill the pairing field so the preview and provider check
                        // show before the joiner commits to Join.
                        ScanTarget.PAIRING_CODE -> joinLauncher.updatePairingCode(
                            code,
                            oauthLinking,
                            oauthLinkingError,
                        )
                    }
                },
                onDismiss = { scanTarget = null },
            )
        }

        launcher.isLinking -> {
            OnboardingProgress(linking = true) { launcher.cancel() }
        }

        joinLauncher.isJoining -> {
            OnboardingProgress(
                linking = false,
                joiningFingerprint = joinLauncher.joiningFingerprint,
            ) { joinLauncher.cancel() }
        }

        showJoin -> {
            JoinLibraryScreen(
                joinLauncher = joinLauncher,
                oauthLinking = oauthLinking,
                oauthLinkingError = oauthLinkingError,
                onRequestScan = { onRequestScan(ScanTarget.PAIRING_CODE) },
                onBack = {
                    joinLauncher.reset()
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

/**
 * The waiting screen for a running attempt: connecting to restore this device's
 * own library when [linking], or joining an existing library otherwise.
 */
@Composable
internal fun OnboardingProgress(
    linking: Boolean,
    joiningFingerprint: String? = null,
    onCancel: () -> Unit,
) {
    if (linking) {
        ProgressScreen(
            R.string.onboarding_connecting_title,
            R.string.onboarding_connecting_body,
            null,
            onCancel,
        )
    } else {
        ProgressScreen(
            R.string.onboarding_joining_title,
            R.string.onboarding_joining_body,
            joiningFingerprint,
            onCancel,
        )
    }
}

@Composable
private fun ProgressScreen(
    @StringRes titleRes: Int,
    @StringRes bodyRes: Int,
    joiningFingerprint: String?,
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
        joiningFingerprint?.let {
            Spacer(modifier = Modifier.height(12.dp))
            Text(
                text = stringResource(R.string.onboarding_join_fingerprint, it),
                style = MaterialTheme.typography.bodyMedium,
                fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                textAlign = TextAlign.Center,
            )
        }
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

/**
 * The first-run welcome screen with no attempt in flight, wrapped in the app
 * chrome — the shared composition the `welcome` screenshot scene and the dev
 * preview below both render. Renders the production [OnboardingIdleContent] with
 * inert callbacks; no session or camera is touched.
 */
@Composable
internal fun WelcomeScene() {
    BaeAppChrome {
        OnboardingIdleContent(
            error = null,
            showPasteDialog = false,
            pasteInput = "",
            callbacks =
                OnboardingIdleCallbacks(
                    onScanQR = {},
                    onShowPasteDialog = {},
                    onPasteInputChange = {},
                    onConnect = {},
                    onDismissPaste = {},
                    onJoinLibrary = {},
                ),
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun WelcomeScenePreview() {
    WelcomeScene()
}

@Preview(showBackground = true)
@Composable
private fun OnboardingProgressLinkingPreview() {
    BaeTheme {
        OnboardingProgress(linking = true, onCancel = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun OnboardingProgressJoiningPreview() {
    BaeTheme {
        OnboardingProgress(linking = false, onCancel = {})
    }
}
