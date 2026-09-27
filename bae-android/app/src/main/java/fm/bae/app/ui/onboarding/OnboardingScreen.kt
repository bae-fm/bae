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
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import fm.bae.app.BaeApp
import fm.bae.app.BaeLogger
import fm.bae.app.OAuthLinker
import fm.bae.app.R
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.QRScannerScreen
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeJoiningDeviceJoinProgress
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

/** Returns a callback that opens the scanner for a [ScanTarget], asking for camera permission first when needed. */
@Composable
private fun rememberScanRequest(
    setError: (ScanTarget, String?) -> Unit,
    onOpen: (ScanTarget) -> Unit,
): (ScanTarget) -> Unit {
    val context = LocalContext.current
    // The target a pending camera-permission request is for.
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
    onScanned: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val instructions =
        when (target) {
            ScanTarget.ANY_SETUP_CODE -> null
            ScanTarget.PAIRING_CODE -> stringResource(R.string.onboarding_join_pairing_instructions)
        }
    QRScannerScreen(
        onScanned = onScanned,
        onDismiss = onDismiss,
        instructions = instructions,
    )
}

@Composable
private fun rememberCodeRouter(
    launcher: LinkLauncher,
    joinLauncher: JoinLauncher,
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    setShowJoin: (Boolean) -> Unit,
): OnboardingCodeRouter =
    remember(launcher, joinLauncher, oauthLinking, oauthLinkingError) {
        OnboardingCodeRouter(
            launcher,
            joinLauncher,
            oauthLinking,
            oauthLinkingError,
            setShowJoin,
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
    // The process-lifetime host built at app launch; restore, join, and cloud
    // sign-in run over it.
    val host = (context.applicationContext as BaeApp).host
    val launcher = remember { LinkLauncher(scope, context, host, onLinked) }
    val joinLauncher = remember { JoinLauncher(scope, context, host, onLinked) }
    var showJoin by remember { mutableStateOf(false) }
    LaunchedEffect(joinLauncher) { showJoin = joinLauncher.resumePending(oauthLinking, oauthLinkingError) }
    // Non-null while the scanner is open, identifying which code it captures.
    var scanTarget by remember { mutableStateOf<ScanTarget?>(null) }
    val codeRouter =
        rememberCodeRouter(launcher, joinLauncher, oauthLinking, oauthLinkingError) { showJoin = it }

    val onRequestScan =
        rememberScanRequest(
            setError = codeRouter::setScanError,
            onOpen = { scanTarget = it },
        )

    when {
        scanTarget != null -> {
            OnboardingScanner(
                target = scanTarget!!,
                onScanned = { code ->
                    scanTarget = null
                    codeRouter.route(code)
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
                joinProgress = joinLauncher.joinProgress,
            ) { joinLauncher.abandonAndReset { showJoin = false } }
        }

        showJoin -> {
            JoinLibraryScreen(
                joinLauncher = joinLauncher,
                oauthLinking = oauthLinking,
                oauthLinkingError = oauthLinkingError,
                onRequestScan = { onRequestScan(ScanTarget.PAIRING_CODE) },
                onBack = {
                    joinLauncher.abandonAndReset { showJoin = false }
                },
            )
        }

        else -> {
            OnboardingIdleScreen(
                launcher = launcher,
                oauthLinking = oauthLinking,
                oauthLinkingError = oauthLinkingError,
                onRequestScan = { onRequestScan(ScanTarget.ANY_SETUP_CODE) },
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
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        Text(text = "bae", style = ThemeText.wordmark.style)
        Spacer(modifier = Modifier.height(ThemeSpace.group))
        Text(
            text = stringResource(R.string.onboarding_tagline),
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(ThemeSpace.page))
        val buttonWidth = Modifier.width(200.dp)
        PrimaryButton(onClick = callbacks.onJoinLibrary, modifier = buttonWidth) {
            Text(stringResource(R.string.onboarding_join_library))
        }
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        OutlinedButton(onClick = callbacks.onScanQR, modifier = buttonWidth) {
            Text(stringResource(R.string.pairing_scan_code))
        }
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        OutlinedButton(onClick = callbacks.onShowPasteDialog, modifier = buttonWidth) {
            Text(stringResource(R.string.onboarding_paste_code))
        }
        if (error != null) {
            Spacer(modifier = Modifier.height(ThemeSpace.related))
            Text(text = error, color = MaterialTheme.colorScheme.error, style = ThemeText.body.style)
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
    joinProgress: BridgeJoiningDeviceJoinProgress? = null,
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
        OnboardingContainer {
            if (joinProgress != null) {
                JoiningDeviceProgress(joinProgress)
            } else {
                CircularProgressIndicator()
                Spacer(modifier = Modifier.height(ThemeSpace.section))
                Text(
                    text = stringResource(R.string.onboarding_joining_title),
                    style = ThemeText.heading.style,
                    textAlign = TextAlign.Center,
                )
            }
            joiningFingerprint?.let {
                Spacer(modifier = Modifier.height(ThemeSpace.group))
                Text(
                    text = stringResource(R.string.onboarding_join_fingerprint, it),
                    style = ThemeText.mono.style,
                    textAlign = TextAlign.Center,
                )
            }
            Spacer(modifier = Modifier.height(ThemeSpace.section))
            OutlinedButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
        }
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
        Spacer(modifier = Modifier.height(ThemeSpace.section))
        Text(
            text = stringResource(titleRes),
            style = ThemeText.heading.style,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        Text(
            text = stringResource(bodyRes),
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        joiningFingerprint?.let {
            Spacer(modifier = Modifier.height(ThemeSpace.group))
            Text(
                text = stringResource(R.string.onboarding_join_fingerprint, it),
                style = ThemeText.mono.style,
                textAlign = TextAlign.Center,
            )
        }
        Spacer(modifier = Modifier.height(ThemeSpace.section))
        OutlinedButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
    }
}

@Composable
private fun OnboardingContainer(content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier.fillMaxSize().padding(ThemeSpace.page),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
        content = content,
    )
}

/** The first-run welcome screen in the app chrome with inert callbacks, for the `welcome` screenshot scene. */
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
