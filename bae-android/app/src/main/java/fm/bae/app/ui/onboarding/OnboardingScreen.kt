package fm.bae.app.ui.onboarding

import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeApp
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.LabeledValueRow
import fm.bae.app.ui.components.PersonNameField
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.QRScannerScreen
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.storageLocationLabel
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.bridgePendingReasonKey

internal val actionWidth = 220.dp

/**
 * Restoring a library over this device's copy: what its code screen says
 * about it, and where going back leaves the person.
 */
internal data class RestoreOver(
    val libraryId: String,
    val title: String,
    val message: String,
    val onBack: () -> Unit,
) : CodeEntry

/**
 * First run on a device: the person chooses to join someone's library or add
 * this device to their own, then scans or types a code; a restore code from
 * one of the person's devices restores here, and an invite joins. With
 * [restoreOver], the person's current restore code restores that library over
 * this device's copy, after another of their devices replaced their keys, or
 * finishes such a restore that stopped partway.
 */
@Composable
internal fun OnboardingScreen(
    oauthProvidersError: String?,
    onBootstrapped: (BridgeBootstrapResult) -> Unit,
    restoreOver: RestoreOver?,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // The process-lifetime host built at app launch; restore and join run over it.
    val host = (context.applicationContext as BaeApp).host
    val flow =
        remember(restoreOver?.libraryId) {
            OnboardingFlow(scope, context, host, oauthProvidersError, restoreOver, onBootstrapped)
        }
    OnboardingSteps(flow)
}

/** The screen for where [flow] stands. */
@Composable
internal fun OnboardingSteps(flow: OnboardingFlow) {
    var scanning by remember { mutableStateOf(false) }
    val requestScan = rememberScanRequest(onDenied = { flow.error = it }, onOpen = { scanning = true })

    when (val step = flow.step) {
        OnboardingStep.Choose -> {
            OnboardingChoices(
                onJoinLibrary = { flow.choose(CodeEntry.JoinLibrary) },
                onAddDevice = { flow.choose(CodeEntry.AddDevice) },
            )
        }

        is OnboardingStep.EnterCode -> {
            if (scanning) {
                QRScannerScreen(
                    onScanned = { code ->
                        scanning = false
                        flow.enter(step, code)
                    },
                    onDismiss = { scanning = false },
                )
            } else {
                val text = codeScreenText(step.entry, onChoices = flow::backToChoices)
                BackHandler(onBack = text.onBack)
                CodeScreen(flow = flow, step = step, text = text, onScan = requestScan)
            }
        }

        is OnboardingStep.Restore -> {
            BackHandler { flow.back(step) }
            RestoreForm(
                step = step,
                error = flow.error,
                onRestore = { flow.restore(step, it) },
                onBack = { flow.back(step) },
            )
        }

        is OnboardingStep.Invite -> {
            BackHandler { flow.back(step) }
            InviteForm(
                step = step,
                error = flow.error,
                onJoin = { flow.join(step, it) },
                onBack = { flow.back(step) },
            )
        }

        is OnboardingStep.Running -> {
            BackHandler { flow.cancel() }
            OnboardingProgress(step = step, onCancel = flow::cancel)
        }
    }
}

/** A new device's first screen: bae's welcome and its two choices. */
@Composable
private fun OnboardingChoices(
    onJoinLibrary: () -> Unit,
    onAddDevice: () -> Unit,
) {
    OnboardingStartColumn(title = null, message = stringResource(R.string.onboarding_tagline)) {
        PrimaryButton(onClick = onAddDevice, modifier = Modifier.width(actionWidth)) {
            Text(stringResource(R.string.onboarding_add_device))
        }
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        OutlinedButton(onClick = onJoinLibrary, modifier = Modifier.width(actionWidth)) {
            Text(stringResource(R.string.onboarding_join_library))
        }
    }
}

/**
 * The start screens' column: bae's icon and wordmark, an optional title, the
 * message, then [actions]. Centered when it fits, and scrolled when it
 * doesn't, such as on a phone held sideways.
 */
@Composable
internal fun OnboardingStartColumn(
    title: String?,
    message: String,
    actions: @Composable ColumnScope.() -> Unit,
) {
    // The scroll keeps the screen's height as the column's minimum, so a
    // column that fits is centered in it.
    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(ThemeSpace.page),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Image(
            painter = painterResource(R.drawable.sheep_icon),
            contentDescription = stringResource(R.string.onboarding_icon_description),
            modifier = Modifier.size(120.dp),
        )
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        Text(text = "bae", style = ThemeText.wordmark.style)
        title?.let {
            Spacer(modifier = Modifier.height(ThemeSpace.group))
            Text(text = it, style = ThemeText.title.style, textAlign = TextAlign.Center)
        }
        Spacer(modifier = Modifier.height(ThemeSpace.group))
        Text(
            text = message,
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(ThemeSpace.page))
        actions()
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
    val needsKey = step.metadata.provider == BridgeCloudProvider.S3 && step.s3Credentials == null
    OnboardingForm(title = stringResource(R.string.onboarding_restore_title), error = error, onBack = onBack) {
        LabeledValueRow(stringResource(R.string.settings_stored_in), storageLocationLabel(step.metadata.location))
        if (step.s3Credentials != null) {
            Text(
                text = stringResource(R.string.onboarding_s3_key_scanned),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
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
                    step.s3Credentials ?: if (needsKey) BridgeS3Credentials(accessKeyId, secretAccessKey) else null,
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
        LabeledValueRow(stringResource(R.string.settings_stored_in), storageLocationLabel(step.metadata.location))
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
        OnboardingChoices(onJoinLibrary = {}, onAddDevice = {})
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
                                    location =
                                        BridgeStorageLocation.Dropbox(
                                            location = BridgeDropboxLocation.SharedFolder(namespaceId = "namespace-1"),
                                            accountLabel = "person@example.com",
                                        ),
                                    oauthAccess = null,
                                ),
                            libraryName = "Household",
                            entry = CodeEntry.JoinLibrary,
                        ),
                    status = BridgeBootstrapStatus.WaitingForApproval,
                ),
            onCancel = {},
        )
    }
}
