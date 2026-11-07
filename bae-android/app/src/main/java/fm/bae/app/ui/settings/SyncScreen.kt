package fm.bae.app.ui.settings

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.SyncLine
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.DeviceResetDialog
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.CardRow
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.RowCard
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.library.LibraryDestination
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgePendingAction
import uniffi.bae_bridge.BridgePendingReason
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgeSaveSyncConfig
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.BridgeSyncFix
import uniffi.bae_bridge.BridgeSyncShown
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone

private val logger = BaeLogger("bae.SyncScreen")

/** The padding of a cause, a fix and the time of the last sync inside the block's card. */
private val BLOCK_PADDING = PaddingValues(start = ThemeSpace.edge, end = ThemeSpace.edge, bottom = ThemeSpace.group)

/** How long ago the preview's last sync was. */
private const val PREVIEW_LAST_SYNCED_SECONDS = 259_200L

/**
 * Runs the action a pending record offers. Restoring with the person's code
 * closes the library for the restore screen.
 */
private class PendingActionRunner(
    private val session: OpenLibrary,
    private val actions: SessionActions,
    private val context: Context,
    private val scope: CoroutineScope,
) {
    var error by mutableStateOf<DisplayError?>(null)

    /** Runs the reconnect a sign-in asks for, with its steps while it runs. */
    val signIns = StorageSetupRunner(context, session, scope, onError = { error = it }, onSetUp = {})

    /** The S3 location a new access key is being asked for; null while none is. */
    var askingForKey by mutableStateOf<BridgeStorageLocation.S3?>(null)

    /** The reset the person is shown the warning of; null while none is. */
    var resetting by mutableStateOf<BridgeDeviceReset?>(null)

    fun perform(
        operation: String,
        action: suspend () -> Unit,
    ) {
        error = null
        scope.launch {
            performBridgeAction(logger, operation, session.errors, { error = it }, action)
        }
    }

    /** Runs the fix a sync state offers. */
    fun runFix(fix: BridgeSyncFix) {
        val handle = session.appHandle
        when (fix) {
            BridgeSyncFix.ResumeUploads -> {
                perform("resume uploads") { handle.setSyncPaused(false) }
            }

            BridgeSyncFix.RetryNow -> {
                session.messages.startRetrySync(handle::retryNow)
            }

            BridgeSyncFix.Reconnect -> {
                perform("reconnect sync") { handle.reconnectSync() }
            }

            BridgeSyncFix.CheckForUpdates -> {
                error = openStoreListing(context)
            }

            is BridgeSyncFix.SignIn -> {
                signIn(fix.provider)
            }

            BridgeSyncFix.EnterAccessKey -> {
                signIn(BridgeCloudProvider.S3)
            }

            BridgeSyncFix.ResetDevice, BridgeSyncFix.RemoveFromThisDevice, BridgeSyncFix.OpenInBrowser -> {
                runDeviceFix(fix)
            }

            // iCloud isn't reachable from Android, so no library here is signed out of it.
            BridgeSyncFix.OpenSystemSettings -> {
                error("$fix is offered for iCloud, which Android doesn't reach")
            }
        }
    }

    /** The fixes that act on this device and its library rather than on sync. */
    private fun runDeviceFix(fix: BridgeSyncFix) {
        when (fix) {
            BridgeSyncFix.ResetDevice -> {
                resetting = AppSessionHolder.prepareDeviceReset(context, session.libraryId)
            }

            BridgeSyncFix.RemoveFromThisDevice -> {
                actions.leaveLibrary()
            }

            BridgeSyncFix.OpenInBrowser -> {
                perform("read the folder to open") {
                    val folder =
                        when (val location = session.appHandle.storageLocation()) {
                            is BridgeStorageLocation.Dropbox -> location.folderLabel
                            is BridgeStorageLocation.OneDrive -> location.folderLabel
                            else -> null
                        }
                    folder?.let { error = openInBrowser(context, it.webUrl) }
                }
            }

            else -> {
                error("$fix is not a fix on this device")
            }
        }
    }

    fun run(action: BridgePendingAction) {
        val handle = session.appHandle
        when (action) {
            BridgePendingAction.Wait, BridgePendingAction.RestoreFile, BridgePendingAction.Review -> {
                error("$action offers nothing to run")
            }

            BridgePendingAction.UpdateApp -> {
                error = openStoreListing(context)
            }

            is BridgePendingAction.SignIn -> {
                signIn(action.provider)
            }

            BridgePendingAction.Reconnect -> {
                perform("reconnect sync") { handle.reconnectSync() }
            }

            is BridgePendingAction.RetryOperation -> {
                perform("retry pending operation ${action.id}") { handle.retryPendingOperation(action.id) }
            }

            is BridgePendingAction.RetryDownload -> {
                perform("pin ${action.releaseId} again") { handle.queuePinReleases(listOf(action.releaseId)) }
            }

            is BridgePendingAction.CancelUpload -> {
                perform("cancel the failed upload ${action.id}") {
                    handle.cancelFailedReleaseMove(action.id)
                }
            }

            BridgePendingAction.RestoreFromCode -> {
                actions.restoreFromCode()
            }
        }
    }

    /**
     * Sign in to the library's storage again: a new access key for an S3 bucket, which the key dialog asks for, or
     * the provider's sign-in.
     */
    private fun signIn(provider: BridgeCloudProvider) {
        if (provider == BridgeCloudProvider.S3) {
            perform("read the S3 location a new access key is for") {
                val location = session.appHandle.storageLocation()
                askingForKey =
                    checkNotNull(location as? BridgeStorageLocation.S3) {
                        "an S3 sign-in record under the storage location $location"
                    }
            }
            return
        }
        val providers = actions.oauthProviders
        if (providers == null) {
            error = DisplayError(context.getString(R.string.oauth_unconfigured))
        } else {
            setUp("sign in to $provider again") { deviceName, setup ->
                providers.signInAgain(session.appHandle, deviceName, setup)
            }
        }
    }

    /** Connect the library's location again with the access key [config] carries. */
    fun signInWithKey(config: BridgeSaveSyncConfig) {
        askingForKey = null
        setUp("sign in to S3 with a new access key") { deviceName, setup ->
            session.appHandle.saveSyncConfig(config, deviceName, setup)
        }
    }

    private fun setUp(
        operation: String,
        setUp: suspend (deviceName: String, setup: BridgeLibrarySetup) -> BridgeStorageSetup,
    ) {
        error = null
        scope.launch { signIns.run(operation, setUp) }
    }
}

/**
 * The Sync page: the state sync is in, with its cause, its fix and when sync last finished; then the library's
 * storage; then everything sync waits on, in plain words, each with its action where the person can act: sign in,
 * retry, resume uploads, restore with the person's code. A wait the person can't help offers nothing; its diagnostic
 * details stay behind Details.
 */
@Composable
internal fun SyncScreen(
    session: OpenLibrary,
    actions: SessionActions,
    onBack: () -> Unit,
    onPush: (LibraryDestination) -> Unit,
) {
    val pending by session.syncStatusStore.pending.collectAsState()
    val line by session.syncStatusStore.line.collectAsState()
    val setupRunning by session.syncStatusStore.setupRunning.collectAsState()
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val runner = remember(session) { PendingActionRunner(session, actions, context, scope) }
    var location by remember(session) { mutableStateOf<BridgeStorageLocation?>(null) }
    var locationRead by remember(session) { mutableStateOf(false) }
    var locationError by remember(session) { mutableStateOf<DisplayError?>(null) }
    // The location is recorded as a setup ends, so it is read again when one stops running.
    LaunchedEffect(session, setupRunning) {
        if (setupRunning) return@LaunchedEffect
        performBridgeAction(logger, "read the library's storage", session.errors, { locationError = it }) {
            location = session.appHandle.storageLocation()
            locationRead = true
            locationError = null
        }
    }

    Column(modifier = Modifier.fillMaxSize()) {
        SettingsTopBar(title = stringResource(R.string.settings_sync), onBack = onBack)
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            item { SyncStatusBlock(line = line, onFix = runner::runFix) }
            item {
                Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                    UndoneSetupNotice(session)
                    runner.signIns.steps?.let { StorageSetupLine(it) }
                    runner.error?.let { ErrorDetailDisclosure(it) }
                    locationError?.let { ErrorDetailDisclosure(it) }
                }
            }
            location?.let { place -> item { StorageSection(place) } }
            if (locationRead && location == null && !setupRunning) {
                item { LocalOnly(onSetUp = { onPush(LibraryDestination.StorageSetup) }) }
            }
            // What is waiting is a local read; nothing shows until it lands.
            pending?.takeIf { it.isNotEmpty() }?.let { records ->
                item { Eyebrow(text = stringResource(R.string.sync_pending)) }
                itemsIndexed(records) { index, record ->
                    if (index > 0) HorizontalDivider()
                    PendingRow(record = record, onAction = runner::run)
                }
            }
        }
    }
    SyncDialogs(runner = runner, actions = actions)
}

/** A library with no storage lives on this device only; storage is set up from here. */
@Composable
private fun LocalOnly(onSetUp: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        Text(stringResource(R.string.settings_local_only))
        Text(
            text = stringResource(R.string.settings_set_up_sync_explanation),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        PrimaryButton(onClick = onSetUp) { Text(stringResource(R.string.storage_setup_title)) }
    }
}

/** A storage setup that ended undone with no setup screen there to hear it: why, and Dismiss. */
@Composable
private fun UndoneSetupNotice(session: OpenLibrary) {
    val scope = rememberScreenScope()
    val undone by session.syncStatusStore.undoneSetup.collectAsState()
    var dismissing by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<DisplayError?>(null) }
    undone?.let { setup ->
        ErrorDetailDisclosure(setup.error)
        TextButton(
            enabled = !dismissing,
            onClick = {
                dismissing = true
                error = null
                scope.launch {
                    performBridgeAction(logger, "dismiss the undone storage setup", session.errors, { error = it }) {
                        session.appHandle.dismissUndoneStorageSetup(setup.operation)
                    }
                    dismissing = false
                }
            },
        ) {
            Text(stringResource(R.string.dismiss))
        }
    }
    error?.let { ErrorDetailDisclosure(it) }
}

/** The dialogs a fix or a pending action opens. */
@Composable
private fun SyncDialogs(
    runner: PendingActionRunner,
    actions: SessionActions,
) {
    runner.askingForKey?.let { place ->
        S3KeyDialog(
            location = place,
            onSignIn = runner::signInWithKey,
            onDismiss = { runner.askingForKey = null },
        )
    }
    runner.resetting?.let { prepared ->
        DeviceResetDialog(
            reset = prepared,
            onConfirm = {
                runner.resetting = null
                actions.resetDevice(prepared)
            },
            onDismiss = { runner.resetting = null },
        )
    }
}

/**
 * The top of the page: the state in words and the color core gives it, then its cause, its fix and when sync last
 * finished, each only when core sends one. Until the status is first read it shows nothing.
 */
@Composable
internal fun SyncStatusBlock(
    line: SyncLine,
    onFix: (BridgeSyncFix) -> Unit,
) {
    val context = LocalContext.current
    when (line) {
        SyncLine.Checking -> {
            Unit
        }

        SyncLine.Unreadable -> {
            RowCard {
                CardRow(
                    title = stringResource(R.string.sync_status),
                    subtitle = stringResource(R.string.settings_sync_unreadable),
                    tone = StatusTone.DANGER,
                )
            }
        }

        is SyncLine.Read -> {
            val shown = line.shown
            RowCard {
                CardRow(
                    title = stringResource(R.string.sync_status),
                    subtitle = syncStatusText(context, shown),
                    tone = shown.tone.statusTone(),
                )
                shown.cause?.let { cause ->
                    Text(
                        text = syncCauseText(context, cause),
                        style = ThemeText.body.style,
                        modifier = Modifier.padding(BLOCK_PADDING),
                    )
                }
                shown.fix?.let { fix ->
                    syncFixLabel(fix)?.let { label ->
                        OutlinedButton(
                            onClick = { onFix(fix) },
                            modifier = Modifier.padding(BLOCK_PADDING),
                        ) {
                            Text(label)
                        }
                    }
                }
                shown.lastSynced?.let { moment ->
                    Text(
                        text = stringResource(R.string.sync_last_synced, syncLastSyncedText(moment)),
                        style = ThemeText.body.style,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(BLOCK_PADDING),
                    )
                }
            }
        }
    }
}

/** The words on a fix's button: this app's own, but for the ones core words. Nothing for a fix Android never gets. */
@Composable
private fun syncFixLabel(fix: BridgeSyncFix): String? {
    val context = LocalContext.current
    return when (fix) {
        BridgeSyncFix.ResumeUploads -> context.coreString("core.pending.action.resume_uploads")

        BridgeSyncFix.RetryNow -> context.coreString("core.activity.retry_now")

        BridgeSyncFix.Reconnect -> context.coreString("core.pending.action.reconnect")

        BridgeSyncFix.CheckForUpdates -> stringResource(R.string.sync_fix_update)

        is BridgeSyncFix.SignIn -> stringResource(R.string.sync_fix_sign_in, cloudProviderLabel(fix.provider))

        BridgeSyncFix.EnterAccessKey -> stringResource(R.string.sync_fix_enter_key)

        BridgeSyncFix.ResetDevice -> context.coreString("core.sync.reset_device.confirm")

        BridgeSyncFix.RemoveFromThisDevice -> stringResource(R.string.sync_fix_remove)

        BridgeSyncFix.OpenInBrowser -> stringResource(R.string.settings_open_in_browser)

        // iCloud signs in from the system's settings, which no Android library is on.
        BridgeSyncFix.OpenSystemSettings -> null
    }
}

/** Open bae's page in the device's app store; the error to show when no store can open it. */
private fun openStoreListing(context: Context): DisplayError? =
    try {
        context.startActivity(
            Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=${context.packageName}"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        null
    } catch (e: ActivityNotFoundException) {
        logger.warning("no app store can open bae's listing", e)
        DisplayError(context.getString(R.string.pending_update_no_store))
    }

/** One record: what it is about, why it waits with the error behind it, what its causes were, and its action. */
@Composable
private fun PendingRow(
    record: BridgePendingRecord,
    onAction: (BridgePendingAction) -> Unit,
) {
    val context = LocalContext.current
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
        Text(text = context.coreString(record.subject), style = ThemeText.strong.style)
        ErrorDetailDisclosure(
            error =
                DisplayError(
                    context.coreString(record.reason.key),
                    record.reason.detail,
                ),
            lineColor = LocalContentColor.current,
        )
        record.reason.causes.forEach { cause ->
            Text(
                text = context.coreString(cause.key),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Row(
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val actionKey = record.actionKey
            when (record.action) {
                // The reason's details show what a review is of.
                BridgePendingAction.Wait, BridgePendingAction.Review -> {
                    Unit
                }

                // The file goes back where it was, on the device that imported it; there is nothing to tap here.
                BridgePendingAction.RestoreFile -> {
                    Text(
                        text = context.coreString(checkNotNull(actionKey) { "restoring a file has a label" }),
                        style = ThemeText.detail.style,
                    )
                }

                else -> {
                    OutlinedButton(onClick = { onAction(record.action) }) {
                        Text(context.coreString(checkNotNull(actionKey) { "${record.action} has a label" }))
                    }
                }
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun SyncScreenPreview() {
    BaeTheme {
        Column(
            modifier = Modifier.padding(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            SyncStatusBlock(
                line =
                    SyncLine.Read(
                        BridgeSyncShown(
                            status = BridgeSyncStatusWord.SIGNED_OUT_OF_DROPBOX,
                            tone = BridgeSyncTone.WARNING,
                            cause = null,
                            fix = BridgeSyncFix.SignIn(BridgeCloudProvider.DROPBOX),
                            lastSynced =
                                java.time.Instant
                                    .now()
                                    .minusSeconds(PREVIEW_LAST_SYNCED_SECONDS),
                        ),
                    ),
                onFix = {},
            )
            listOf(
                BridgePendingRecord(
                    subject = BridgeMessage(key = "core.pending.subject.connection", args = emptyList()),
                    reason =
                        BridgePendingReason(
                            key = "core.pending.reason.provider_pending",
                            detail = null,
                            causes = emptyList(),
                        ),
                    action = BridgePendingAction.SignIn(BridgeCloudProvider.DROPBOX),
                    actionKey = "core.pending.action.sign_in",
                ),
                BridgePendingRecord(
                    subject = BridgeMessage(key = "core.pending.operation.invite", args = emptyList()),
                    reason =
                        BridgePendingReason(
                            "core.pending.reason.failed",
                            "S3: Network: S3 request failed: dispatch failure: io error: connection refused",
                            emptyList(),
                        ),
                    action = BridgePendingAction.RetryOperation(id = 7L),
                    actionKey = "core.pending.action.retry",
                ),
            ).forEach { PendingRow(record = it, onAction = {}) }
        }
    }
}
