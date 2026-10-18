package fm.bae.app.ui.onboarding

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import fm.bae.app.BaeApp
import fm.bae.app.BaeLogger
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.LabeledValueRow
import fm.bae.app.ui.components.PersonNameField
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.QRScannerScreen
import fm.bae.app.ui.components.S3KeyFields
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.bridgePendingReasonKey

private const val TAG = "bae.OnboardingScreen"
private val logger = BaeLogger(TAG)

private val actionWidth = 220.dp

private class OnboardingStartCallbacks(
    val onScan: () -> Unit,
    val onShowCodeDialog: () -> Unit,
    val onCodeInputChange: (String) -> Unit,
    val onEnterCode: (String) -> Unit,
    val onDismissCodeDialog: () -> Unit,
)

/**
 * First run on a device: a restore code from one of the person's devices, or
 * an invite to someone's library, scanned or typed, restores or joins here.
 */
@Composable
fun OnboardingScreen(
    oauthProvidersError: String?,
    onBootstrapped: (BridgeBootstrapResult) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // The process-lifetime host built at app launch; restore and join run over it.
    val host = (context.applicationContext as BaeApp).host
    val flow = remember { OnboardingFlow(scope, context, host, oauthProvidersError, onBootstrapped) }
    var scanning by remember { mutableStateOf(false) }
    val requestScan = rememberScanRequest(onDenied = { flow.error = it }, onOpen = { scanning = true })

    when (val step = flow.step) {
        OnboardingStep.Start -> {
            if (scanning) {
                QRScannerScreen(
                    onScanned = { code ->
                        scanning = false
                        flow.enter(code)
                    },
                    onDismiss = { scanning = false },
                )
            } else {
                OnboardingStart(flow = flow, onScan = requestScan)
            }
        }

        is OnboardingStep.Restore -> {
            BackHandler { flow.back() }
            RestoreForm(
                step = step,
                error = flow.error,
                onRestore = { flow.restore(step, it) },
                onBack = flow::back,
            )
        }

        is OnboardingStep.Invite -> {
            BackHandler { flow.back() }
            InviteForm(
                step = step,
                error = flow.error,
                onJoin = { flow.join(step, it) },
                onBack = flow::back,
            )
        }

        is OnboardingStep.Running -> {
            BackHandler { flow.cancel() }
            OnboardingProgress(step = step, onCancel = flow::cancel)
        }
    }
}

/** Returns a callback that opens the scanner, asking for camera permission first when needed. */
@Composable
private fun rememberScanRequest(
    onDenied: (String) -> Unit,
    onOpen: () -> Unit,
): () -> Unit {
    val context = LocalContext.current
    val permissionLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) {
                onOpen()
            } else {
                logger.info("camera permission denied; the code can still be typed")
                onDenied(context.getString(R.string.onboarding_camera_permission_required))
            }
        }
    return {
        val granted =
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED
        if (granted) onOpen() else permissionLauncher.launch(Manifest.permission.CAMERA)
    }
}

@Composable
private fun OnboardingStart(
    flow: OnboardingFlow,
    onScan: () -> Unit,
) {
    var showCodeDialog by remember { mutableStateOf(false) }
    var codeInput by remember { mutableStateOf("") }
    OnboardingStartContent(
        error = flow.error,
        showCodeDialog = showCodeDialog,
        codeInput = codeInput,
        callbacks =
            OnboardingStartCallbacks(
                onScan = {
                    flow.error = null
                    onScan()
                },
                onShowCodeDialog = {
                    flow.error = null
                    codeInput = ""
                    showCodeDialog = true
                },
                onCodeInputChange = { codeInput = it },
                onEnterCode = { code ->
                    showCodeDialog = false
                    flow.enter(code)
                },
                onDismissCodeDialog = { showCodeDialog = false },
            ),
    )
}

@Composable
private fun OnboardingStartContent(
    error: String?,
    showCodeDialog: Boolean,
    codeInput: String,
    callbacks: OnboardingStartCallbacks,
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
        PrimaryButton(onClick = callbacks.onScan, modifier = Modifier.width(actionWidth)) {
            Text(stringResource(R.string.onboarding_scan_code))
        }
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        OutlinedButton(onClick = callbacks.onShowCodeDialog, modifier = Modifier.width(actionWidth)) {
            Text(stringResource(R.string.onboarding_enter_code))
        }
        if (error != null) {
            Spacer(modifier = Modifier.height(ThemeSpace.related))
            ErrorText(error)
        }
        Spacer(modifier = Modifier.weight(1f))
    }
    if (showCodeDialog) {
        PasteCodeDialog(
            text =
                PasteDialogText(
                    title = stringResource(R.string.onboarding_enter_code),
                    instructions = stringResource(R.string.onboarding_enter_code_instructions),
                    placeholder = stringResource(R.string.onboarding_code_placeholder),
                    confirmLabel = stringResource(R.string.continue_action),
                ),
            pasteInput = codeInput,
            onInputChange = callbacks.onCodeInputChange,
            onConfirm = callbacks.onEnterCode,
            onDismiss = callbacks.onDismissCodeDialog,
        )
    }
}

/**
 * The person's own restore code, read: S3 asks for the bucket's access key;
 * the account providers sign in as the restore runs.
 */
@Composable
private fun RestoreForm(
    step: OnboardingStep.Restore,
    error: String?,
    onRestore: (BridgeS3Credentials?) -> Unit,
    onBack: () -> Unit,
) {
    var accessKeyId by remember { mutableStateOf("") }
    var secretAccessKey by remember { mutableStateOf("") }
    val needsKey = step.metadata.provider == BridgeCloudProvider.S3
    OnboardingForm(title = stringResource(R.string.onboarding_restore_title), error = error, onBack = onBack) {
        LabeledValueRow(stringResource(R.string.settings_provider), cloudProviderLabel(step.metadata.provider))
        if (needsKey) {
            Text(
                text = stringResource(R.string.onboarding_s3_key_explanation),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            S3KeyFields(
                accessKeyId = accessKeyId,
                secretAccessKey = secretAccessKey,
                onAccessKeyIdChange = { accessKeyId = it.trim() },
                onSecretAccessKeyChange = { secretAccessKey = it.trim() },
            )
        }
        PrimaryButton(
            onClick = {
                onRestore(
                    if (needsKey) BridgeS3Credentials(accessKeyId, secretAccessKey) else null,
                )
            },
            enabled = !needsKey || (accessKeyId.isNotEmpty() && secretAccessKey.isNotEmpty()),
            modifier = Modifier.width(actionWidth),
        ) {
            Text(stringResource(R.string.onboarding_restore_action))
        }
    }
}

/** An invite, read: the library it names, and the name the person joins under. */
@Composable
private fun InviteForm(
    step: OnboardingStep.Invite,
    error: String?,
    onJoin: (String) -> Unit,
    onBack: () -> Unit,
) {
    var personName by remember { mutableStateOf("") }
    OnboardingForm(
        title = stringResource(R.string.onboarding_join_title_named, step.libraryName),
        error = error,
        onBack = onBack,
    ) {
        LabeledValueRow(stringResource(R.string.settings_provider), cloudProviderLabel(step.metadata.provider))
        PersonNameField(personName) { personName = it }
        PrimaryButton(
            onClick = { onJoin(personName.trim()) },
            enabled = personName.isNotBlank(),
            modifier = Modifier.width(actionWidth),
        ) {
            Text(stringResource(R.string.onboarding_join_action))
        }
    }
}

@Composable
private fun OnboardingForm(
    title: String,
    error: String?,
    onBack: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(ThemeSpace.page).verticalScroll(rememberScrollState()),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.group),
    ) {
        Text(text = title, style = ThemeText.title.style, textAlign = TextAlign.Center)
        content()
        error?.let { ErrorText(it) }
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }
}

/** A restore or join running: what it waits on now, and Cancel. */
@Composable
internal fun OnboardingProgress(
    step: OnboardingStep.Running,
    onCancel: () -> Unit,
) {
    val context = LocalContext.current
    val title =
        when (val from = step.from) {
            is OnboardingStep.Invite -> stringResource(R.string.onboarding_joining_library, from.libraryName)
            is OnboardingStep.Restore -> stringResource(R.string.onboarding_restoring_title)
        }
    OnboardingContainer {
        CircularProgressIndicator()
        Spacer(modifier = Modifier.height(ThemeSpace.section))
        Text(text = title, style = ThemeText.heading.style, textAlign = TextAlign.Center)
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        val lines =
            when (val status = step.status) {
                null, BridgeBootstrapStatus.Connecting -> {
                    listOf(stringResource(R.string.onboarding_status_connecting))
                }

                BridgeBootstrapStatus.WaitingForApproval -> {
                    listOf(stringResource(R.string.onboarding_status_waiting_for_approval))
                }

                BridgeBootstrapStatus.Loading -> {
                    listOf(stringResource(R.string.onboarding_status_loading))
                }

                is BridgeBootstrapStatus.Pending -> {
                    status.records.map { context.coreString(bridgePendingReasonKey(it.reason.kind)) }
                }
            }
        lines.forEach { line ->
            Text(
                text = line,
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
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
        OnboardingStartContent(
            error = null,
            showCodeDialog = false,
            codeInput = "",
            callbacks =
                OnboardingStartCallbacks(
                    onScan = {},
                    onShowCodeDialog = {},
                    onCodeInputChange = {},
                    onEnterCode = {},
                    onDismissCodeDialog = {},
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
private fun OnboardingProgressWaitingPreview() {
    BaeTheme {
        OnboardingProgress(
            step =
                OnboardingStep.Running(
                    from =
                        OnboardingStep.Invite(
                            code = "invite-code",
                            metadata =
                                BridgeCodeMetadata(
                                    libraryId = "library-1",
                                    memberId = "member-1",
                                    provider = BridgeCloudProvider.DROPBOX,
                                    oauthAccess = null,
                                ),
                            libraryName = "Household",
                        ),
                    status = BridgeBootstrapStatus.WaitingForApproval,
                ),
            onCancel = {},
        )
    }
}
