package fm.bae.app.ui.onboarding

import android.text.format.DateUtils
import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
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
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.LabeledValueRow
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.QRScannerScreen
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.storageLocationLabel
import kotlinx.coroutines.delay
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeInviteDeliveryKind
import uniffi.bae_bridge.BridgeInviteInfo
import uniffi.bae_bridge.BridgeNextCheck
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeStorageLocation
import java.time.Duration
import java.time.Instant

internal val actionWidth = 220.dp

/** How often a waiting join's countdown redraws. */
private const val COUNTDOWN_TICK_MS = 1_000L

private const val PREVIEW_WAIT_SECONDS = 42L

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
 * First run on a device: the person scans or types a code; a restore code from
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
    val scope = rememberScreenScope()
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
    val requestScan = rememberScanRequest(onDenied = { flow.error = DisplayError(it) }, onOpen = { scanning = true })

    when (val step = flow.step) {
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
                val text = codeScreenText(step.entry)
                // The welcome screen leaves Back to the system.
                text?.let { BackHandler(onBack = it.onBack) }
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

        is OnboardingStep.Running -> {
            // Once a join's request may have been sent there is no cancel:
            // Back leaves the screen, and the join goes on at the next launch.
            val cancellable = step.cancellable()
            BackHandler(enabled = cancellable) { flow.cancel() }
            OnboardingProgress(
                step = step,
                onCancel = flow::cancel.takeIf { cancellable },
                onCheckNow = flow::checkNow,
            )
        }
    }
}

/**
 * The start screens' column: bae's icon and wordmark, an optional title and
 * message, then [actions]. Centered when it fits, and scrolled when it
 * doesn't, such as on a phone held sideways.
 */
@Composable
internal fun OnboardingStartColumn(
    title: String?,
    message: String?,
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
        message?.let {
            Spacer(modifier = Modifier.height(ThemeSpace.group))
            Text(
                text = it,
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
        }
        Spacer(modifier = Modifier.height(ThemeSpace.page))
        actions()
    }
}

/**
 * What a restore says on its form and while it runs: on a new device it adds
 * this device to the person's own library; over this device's copy it
 * restores that library.
 */
private class RestoreText(
    @StringRes val title: Int,
    @StringRes val action: Int,
    @StringRes val running: Int,
) {
    companion object {
        fun of(entry: CodeEntry): RestoreText =
            when (entry) {
                CodeEntry.NewDevice -> {
                    RestoreText(
                        title = R.string.onboarding_add_device,
                        action = R.string.onboarding_add_action,
                        running = R.string.onboarding_adding_device,
                    )
                }

                is RestoreOver -> {
                    RestoreText(
                        title = R.string.onboarding_restore_title,
                        action = R.string.onboarding_restore_action,
                        running = R.string.onboarding_restoring_title,
                    )
                }
            }
    }
}

/**
 * The person's own restore code, read: S3 asks for the bucket's access key;
 * the account providers sign in as the restore runs.
 */
@Composable
internal fun RestoreForm(
    step: OnboardingStep.Restore,
    error: DisplayError?,
    onRestore: (BridgeS3Credentials?) -> Unit,
    onBack: () -> Unit,
) {
    var accessKeyId by remember { mutableStateOf("") }
    var secretAccessKey by remember { mutableStateOf("") }
    val needsKey = step.metadata.provider == BridgeCloudProvider.S3 && step.s3Credentials == null
    val text = RestoreText.of(step.entry)
    OnboardingForm(title = stringResource(text.title), error = error, onBack = onBack) {
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
            Text(stringResource(text.action))
        }
    }
}

@Composable
private fun OnboardingForm(
    title: String,
    error: DisplayError?,
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
        error?.let { ErrorDetailDisclosure(it) }
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }
}

/**
 * A restore can be cancelled until it ends; a join only until its request
 * may have been sent, which coven reports as waiting for approval.
 */
internal fun OnboardingStep.Running.cancellable(): Boolean =
    when (from) {
        is OnboardingStep.Restore -> true
        is OnboardingStep.Invite -> status == null || status is BridgeBootstrapStatus.Connecting
    }

/**
 * A restore or join running: what it waits on now, and Cancel while it can
 * be. A join waiting for approval shows a sent invite's confirmation number,
 * when it checks next, and Check now.
 */
@Composable
internal fun OnboardingProgress(
    step: OnboardingStep.Running,
    onCancel: (() -> Unit)?,
    onCheckNow: () -> Unit,
) {
    val context = LocalContext.current
    val title =
        when (val from = step.from) {
            is OnboardingStep.Invite -> stringResource(R.string.onboarding_joining_library, from.invite.libraryName)
            is OnboardingStep.Restore -> stringResource(RestoreText.of(from.entry).running)
        }
    OnboardingContainer {
        val status = step.status
        if (status !is BridgeBootstrapStatus.WaitingForApproval) {
            CircularProgressIndicator()
            Spacer(modifier = Modifier.height(ThemeSpace.section))
        }
        Text(text = title, style = ThemeText.heading.style, textAlign = TextAlign.Center)
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        when (status) {
            is BridgeBootstrapStatus.WaitingForApproval -> {
                WaitingForApproval(status = status, onCheckNow = onCheckNow)
            }

            else -> {
                val lines =
                    when (status) {
                        null, BridgeBootstrapStatus.Connecting -> {
                            listOf(stringResource(R.string.onboarding_status_connecting))
                        }

                        BridgeBootstrapStatus.Loading -> {
                            listOf(stringResource(R.string.onboarding_status_loading))
                        }

                        is BridgeBootstrapStatus.Pending -> {
                            status.records.map { context.coreString(it.reason.key) }
                        }

                        is BridgeBootstrapStatus.WaitingForApproval -> {
                            emptyList()
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
            }
        }
        onCancel?.let {
            Spacer(modifier = Modifier.height(ThemeSpace.section))
            OutlinedButton(onClick = it) { Text(stringResource(R.string.cancel)) }
        }
    }
}

/**
 * A join waiting for approval: a sent invite's confirmation number, a
 * countdown to the next check, and Check now, which a provider's cooldown
 * holds.
 */
@Composable
private fun WaitingForApproval(
    status: BridgeBootstrapStatus.WaitingForApproval,
    onCheckNow: () -> Unit,
) {
    status.confirmation?.let { number ->
        Text(
            text = stringResource(R.string.join_request_confirmation),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(text = number, style = ThemeText.hero.style)
        Spacer(modifier = Modifier.height(ThemeSpace.related))
    }
    val due =
        when (val next = status.nextCheck) {
            BridgeNextCheck.Checking -> null
            is BridgeNextCheck.At -> next.time
            is BridgeNextCheck.CoolingDown -> next.until
        }
    val countdown =
        if (due == null) {
            stringResource(R.string.onboarding_checking)
        } else {
            val remaining = rememberSecondsUntil(due)
            stringResource(R.string.onboarding_next_check, DateUtils.formatElapsedTime(remaining))
        }
    Text(text = countdown, style = ThemeText.body.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
    Spacer(modifier = Modifier.height(ThemeSpace.section))
    OutlinedButton(onClick = onCheckNow, enabled = status.nextCheck is BridgeNextCheck.At) {
        Text(stringResource(R.string.onboarding_check_now))
    }
}

/** The whole seconds left until [due], counting down each second. */
@Composable
private fun rememberSecondsUntil(due: Instant): Long {
    var now by remember { mutableStateOf(Instant.now()) }
    LaunchedEffect(due) {
        while (true) {
            now = Instant.now()
            delay(COUNTDOWN_TICK_MS)
        }
    }
    return Duration.between(now, due).seconds.coerceAtLeast(0)
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

@Preview(showBackground = true)
@Composable
private fun OnboardingProgressWaitingPreview() {
    BaeTheme {
        OnboardingProgress(
            step =
                OnboardingStep.Running(
                    from =
                        OnboardingStep.Invite(
                            start = JoinStart.Code("invite-code"),
                            invite =
                                BridgeInviteInfo(
                                    metadata =
                                        BridgeCodeMetadata(
                                            libraryId = "library-1",
                                            memberId = "member-1",
                                            provider = BridgeCloudProvider.DROPBOX,
                                            location =
                                                BridgeStorageLocation.Dropbox(
                                                    location =
                                                        BridgeDropboxLocation.SharedFolder(namespaceId = "namespace-1"),
                                                    accountLabel = "person@example.com",
                                                ),
                                            oauthAccess = null,
                                        ),
                                    libraryName = "Household",
                                    invitee = "Dan",
                                    delivery = BridgeInviteDeliveryKind.SENT,
                                ),
                            entry = CodeEntry.NewDevice,
                        ),
                    status =
                        BridgeBootstrapStatus.WaitingForApproval(
                            confirmation = "041 778",
                            nextCheck = BridgeNextCheck.At(Instant.now().plusSeconds(PREVIEW_WAIT_SECONDS)),
                        ),
                ),
            onCancel = null,
            onCheckNow = {},
        )
    }
}
