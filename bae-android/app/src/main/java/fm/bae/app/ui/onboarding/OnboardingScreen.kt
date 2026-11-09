package fm.bae.app.ui.onboarding

import android.text.format.DateUtils
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
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
import fm.bae.app.data.LibraryOpenProgress
import fm.bae.app.data.LibrarySetupStatus
import fm.bae.app.displayError
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.OpenProgressLine
import fm.bae.app.ui.components.QRScannerScreen
import fm.bae.app.ui.components.WaitingIndicator
import fm.bae.app.ui.countdownSecondsLeft
import fm.bae.app.ui.rememberCountdownNow
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.settings.inviteExpiry
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import uniffi.bae_bridge.BootstrapCancellation
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeInviteInfo
import uniffi.bae_bridge.BridgeNextCheck
import uniffi.bae_bridge.BridgeStorageLocation
import java.time.Instant

internal val actionWidth = 220.dp

private const val PREVIEW_WAIT_SECONDS = 42L
private const val PREVIEW_INVITE_SECONDS = 86_400L

/**
 * Restoring a library over this device's copy: what its code screen says
 * about it, where going back leaves the person, and what the screen does as
 * it opens.
 */
internal data class RestoreOver(
    val libraryId: String,
    val title: String,
    val message: String,
    val onBack: () -> Unit,
    val start: Start,
) : CodeEntry {
    /**
     * What the screen does as it opens: wait for the person's current
     * restore code, or finish a restore over this copy that stopped partway,
     * from what core kept and with no code, which the code then also does.
     */
    enum class Start { CODE, FINISH }
}

/**
 * First run on a device: the person scans or types a code; a restore code from
 * one of the person's devices restores here, and an invite joins. The welcome
 * lists the libraries this device holds, which [libraryActions] opens or
 * leaves to finish removing. With [restoreOver], the person's current restore
 * code restores that library over this device's copy, after another of their
 * devices replaced their keys, or finishes such a restore that stopped
 * partway; that screen lists no libraries.
 */
@Composable
internal fun OnboardingScreen(
    oauthProvidersError: String?,
    onBootstrapped: (BridgeBootstrapResult) -> Unit,
    restoreOver: RestoreOver?,
    libraryActions: WelcomeLibraryActions? = null,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    // The process-lifetime host and bug reports built at app launch; restore and
    // join run over them.
    val app = context.applicationContext as BaeApp
    val flow =
        remember(restoreOver?.libraryId) {
            OnboardingFlow(
                scope,
                context,
                OnboardingCore(
                    app.host,
                    ::BootstrapCancellation,
                    { LibraryOpenProgress(app.host, scope, app.bugs) },
                    { LibrarySetupStatus.of(app.host, scope, app.bugs) },
                ),
                oauthProvidersError,
                restoreOver,
                onBootstrapped,
            )
        }
    val actions by rememberUpdatedState(libraryActions)
    val libraries =
        remember(restoreOver?.libraryId) {
            if (restoreOver != null) {
                null
            } else {
                WelcomeLibraries(
                    scope,
                    WelcomeLibrariesCore.live(app.host, scope, app.bugs),
                    errorDisplay = { context.displayError(it) },
                    removalFailureLine = { failure ->
                        failure.copy(line = "${context.coreString("ui.load_error.removal.title")}: ${failure.line}")
                    },
                    onFilesRemain = { actions?.onFilesRemain?.invoke(it) },
                )
            }
        }
    OnboardingSteps(
        flow,
        libraries = libraries?.let { model -> { WelcomeLibrariesSection(model) { actions?.onOpen?.invoke(it) } } },
    )
}

/**
 * The screen for where [flow] stands. The welcome shows [libraries], the list
 * of this device's libraries, above its actions.
 */
@Composable
internal fun OnboardingSteps(
    flow: OnboardingFlow,
    libraries: (@Composable () -> Unit)? = null,
) {
    var scanning by remember { mutableStateOf(false) }
    val requestScan = rememberScanRequest(onDenied = { flow.error = DisplayError(it) }, onOpen = { scanning = true })

    when (val step = flow.step) {
        is OnboardingStep.EnterCode -> {
            if (scanning) {
                QRScannerScreen(
                    onScanned = { code ->
                        scanning = false
                        flow.enter(step, code, CodeSource.SCANNED)
                    },
                    onDismiss = { scanning = false },
                )
            } else {
                val text = codeScreenText(step.entry)
                // The welcome screen leaves Back to the system.
                text?.let { BackHandler(onBack = it.onBack) }
                CodeScreen(flow = flow, step = step, text = text, onScan = requestScan, libraries = libraries)
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

        is OnboardingStep.RestoreSaved -> {
            BackHandler { flow.back(step) }
            SavedRestoreForm(
                step = step,
                error = flow.error,
                onRestore = { flow.restore(step, it) },
                onBack = { flow.back(step) },
            )
        }

        is OnboardingStep.Running -> {
            // Once a join's request may have been sent there is no cancel:
            // Back leaves the screen, and the join goes on at the next launch.
            val cancellable = step.state?.cancellable == true
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
        Text(text = "bae", style = ThemeText.wordmark.style, color = BaeTheme.colors.logoWord)
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
 * A restore or join running: what it waits on now, and Cancel while it can
 * be. A join waiting for approval shows its confirmation number,
 * when it checks next, and Check now.
 */
@Composable
internal fun OnboardingProgress(
    step: OnboardingStep.Running,
    onCancel: (() -> Unit)?,
    onCheckNow: () -> Unit,
    nowMs: () -> Long = System::currentTimeMillis,
) {
    val title =
        when (val from = step.from) {
            is OnboardingStep.Invite -> stringResource(R.string.onboarding_joining_library, from.invite.libraryName)
            is OnboardingStep.Restore -> stringResource(RestoreText.of(from.entry).running)
            is OnboardingStep.RestoreSaved -> stringResource(RestoreText.RESTORE.running)
            is OnboardingStep.FinishRestore -> stringResource(RestoreText.RESTORE.running)
            is OnboardingStep.ResumeRestore -> stringResource(RestoreText.ADD.running)
        }
    OnboardingContainer {
        val status = step.state?.status
        // While the restore's open runs a step, its line stands in for the
        // spinner and the status, which it has moved past.
        var stepping by remember(step.openSteps) { mutableStateOf(false) }
        // A counted step's own bar stands in for the spinner.
        var counting by remember(step.steps) { mutableStateOf(false) }
        if (status !is BridgeBootstrapStatus.WaitingForApproval) {
            OpenProgressLine(
                steps = step.openSteps,
                modifier = Modifier.widthIn(max = STEP_LINE_MAX_WIDTH),
                onStepping = { stepping = it },
            ) {
                if (!counting) WaitingIndicator()
            }
            Spacer(modifier = Modifier.height(ThemeSpace.section))
        }
        Text(text = title, style = ThemeText.heading.style, textAlign = TextAlign.Center)
        Spacer(modifier = Modifier.height(ThemeSpace.related))
        when (status) {
            is BridgeBootstrapStatus.WaitingForApproval -> {
                // Only a join waits for approval.
                WaitingForApproval(
                    status = status,
                    expiresAt = (step.from as OnboardingStep.Invite).invite.expiresAt,
                    onCheckNow = onCheckNow,
                    nowMs = nowMs,
                )
            }

            else -> {
                // The open's line has moved past the status.
                if (!stepping) {
                    BootstrapStepLine(
                        steps = step.steps,
                        modifier = Modifier.widthIn(max = STEP_LINE_MAX_WIDTH),
                        onStepping = { counting = it },
                    ) {
                        StatusLines(status)
                    }
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
 * A join waiting for approval: until when it waits, which is when its invite
 * expires, its confirmation number, a countdown to the next
 * check, and Check now, which a provider's cooldown holds.
 */
@Composable
private fun WaitingForApproval(
    status: BridgeBootstrapStatus.WaitingForApproval,
    expiresAt: Instant,
    onCheckNow: () -> Unit,
    nowMs: () -> Long,
) {
    val context = LocalContext.current
    val until = remember(expiresAt) { context.inviteExpiry(expiresAt) }
    Text(
        text = stringResource(R.string.onboarding_waiting_until, until),
        style = ThemeText.body.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
    )
    Spacer(modifier = Modifier.height(ThemeSpace.related))
    Text(
        text = stringResource(R.string.join_request_confirmation),
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Text(text = status.confirmation, style = ThemeText.hero.style)
    Spacer(modifier = Modifier.height(ThemeSpace.related))
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
            val dueMs = due.toEpochMilli()
            val remaining = countdownSecondsLeft(dueMs, rememberCountdownNow(dueMs, nowMs))
            stringResource(R.string.onboarding_next_check, DateUtils.formatElapsedTime(remaining))
        }
    Text(text = countdown, style = ThemeText.body.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
    Spacer(modifier = Modifier.height(ThemeSpace.section))
    OutlinedButton(onClick = onCheckNow, enabled = status.nextCheck is BridgeNextCheck.At) {
        Text(stringResource(R.string.onboarding_check_now))
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
                                                    folderLabel = null,
                                                ),
                                            oauthAccess = null,
                                            carriesS3Key = false,
                                        ),
                                    libraryName = "Household",
                                    invitee = "Dan",
                                    expiresAt = Instant.now().plusSeconds(PREVIEW_INVITE_SECONDS),
                                ),
                            entry = CodeEntry.NewDevice,
                        ),
                    state =
                        BridgeBootstrapState(
                            status =
                                BridgeBootstrapStatus.WaitingForApproval(
                                    confirmation = "041 778",
                                    nextCheck = BridgeNextCheck.At(Instant.now().plusSeconds(PREVIEW_WAIT_SECONDS)),
                                ),
                            cancellable = false,
                        ),
                    steps = LatestStream.bootstrapSteps().only,
                ),
            onCancel = null,
            onCheckNow = {},
        )
    }
}
