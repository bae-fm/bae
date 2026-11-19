package fm.bae.app.ui.settings

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.PreparedReset
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.SyncLine
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.DeviceResetDialog
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.cloudProviderLabel
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.joinsTileAbove
import fm.bae.app.ui.rememberCountdownLine
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.storageLocationLabel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.BridgeSyncCause
import uniffi.bae_bridge.BridgeSyncCountdown
import uniffi.bae_bridge.BridgeSyncFix

private val logger = BaeLogger("bae.SyncScreen")

/** The padding of a cause inside the status card: the line under the status, then 10dp before what follows. */
private val CAUSE_PADDING =
    PaddingValues(start = ThemeSpace.edge, top = ThemeSpace.line, end = ThemeSpace.edge, bottom = 10.dp)

/** The padding of the fix button, which the design sets 14dp above the card's foot. */
private val FIX_PADDING =
    PaddingValues(start = ThemeSpace.edge, top = ThemeSpace.line, end = ThemeSpace.edge, bottom = 14.dp)

/** The padding of the time of the last sync, the card's last line. */
private val LAST_SYNCED_PADDING = PaddingValues(start = ThemeSpace.edge, end = ThemeSpace.edge, bottom = 14.dp)

/**
 * Runs the fix a sync state offers. Restoring with the person's code
 * closes the library for the restore screen.
 */
private class SyncFixRunner(
    private val session: OpenLibrary,
    private val actions: SessionActions,
    private val context: Context,
    private val scope: CoroutineScope,
) {
    var error by mutableStateOf<DisplayError?>(null)

    /** Runs the reconnect a sign-in asks for, with its steps while it runs. */
    val signIns = StorageSetupRunner(context, session, scope, onError = { error = it })

    /** The S3 key dialog, which an S3 sign-in opens as Replace key does. */
    val keyReplacement =
        S3KeyReplacement(
            scope = scope,
            useAccessKey = session.appHandle::useAccessKey,
            display = session.errors::display,
        )

    /** The reset the person is shown the warning of; null while none is. */
    var resetting by mutableStateOf<PreparedReset?>(null)

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
                keyReplacement.open()
            }

            BridgeSyncFix.EnterRestoreCode -> {
                actions.restoreFromCode()
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
                perform("prepare the device reset") {
                    resetting = AppSessionHolder.prepareDeviceReset(context, session.libraryId)
                }
            }

            BridgeSyncFix.RemoveFromThisDevice -> {
                actions.leaveLibrary()
            }

            BridgeSyncFix.OpenInBrowser -> {
                error = null
                val folder =
                    when (val location = session.configStore.config.value.storage) {
                        is BridgeStorageLocation.Dropbox -> location.folderLabel
                        is BridgeStorageLocation.OneDrive -> location.folderLabel
                        else -> null
                    }
                folder?.let { error = openInBrowser(context, it.webUrl) }
            }

            else -> {
                error("$fix is not a fix on this device")
            }
        }
    }

    /**
     * Sign in to the library's storage again: a new S3 key, which the S3 key dialog asks for, or the provider's
     * sign-in.
     */
    private fun signIn(provider: BridgeCloudProvider) {
        if (provider == BridgeCloudProvider.S3) {
            keyReplacement.open()
            return
        }
        val providers = actions.oauthProviders
        if (providers == null) {
            error = DisplayError(context.getString(R.string.oauth_unconfigured))
        } else {
            setUp("sign in to $provider again") { setup ->
                providers.signInAgain(session.appHandle, setup)
            }
        }
    }

    private fun setUp(
        operation: String,
        setUp: suspend (setup: BridgeLibrarySetup) -> BridgeStorageSetup,
    ) {
        error = null
        scope.launch { signIns.run(operation, setUp) }
    }
}

/**
 * The Sync page: the state sync is in, with its cause, its fix and when sync last finished; then the library's
 * storage and its restore code.
 */
@Composable
internal fun SyncScreen(
    session: OpenLibrary,
    actions: SessionActions,
    onBack: () -> Unit,
) {
    val line by session.syncStatusStore.line.collectAsState()
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val runner = remember(session) { SyncFixRunner(session, actions, context, scope) }
    val config by session.configStore.config.collectAsState()
    val location = config.storage

    // The restore code is made once per visit, whether or not its section is on screen.
    val restoreCode =
        rememberRestoreCodeReveal(
            enabled = location != null,
            generate = { session.appHandle.generateRestoreCode() },
        )

    val locationLabel = location?.let { storageLocationLabel(it) }
    AndroidSettingsSync(
        line = line,
        notices = {
            UndoneSetupNotice(session)
            runner.signIns.steps?.let { StorageSetupLine(it) }
            runner.error?.let { AndroidNotice(it) }
        },
        location = location,
        access = config.storageAccess,
        keyLine = { runner.keyReplacement.line },
        onReplaceKey = runner.keyReplacement::open,
        onDismissKeyLine = runner.keyReplacement::dismissLine,
        restoreCode = restoreCode.state,
        onBack = onBack,
        onFix = runner::runFix,
        onShowRestoreCode = restoreCode::show,
        onHideRestoreCode = restoreCode::hide,
        onPrintRestoreCode = { code ->
            locationLabel?.let { label -> printRestoreCode(context, RestoreCodePage.of(context, code, label)) }
        },
    )
    SyncDialogs(runner = runner, actions = actions)
}

/**
 * The Sync page over what it shows: the state of sync with its fix, [notices] of setups and failed actions, then the
 * library's [location] with the access key the library records for the person, [access], and its restore code.
 * [keyLine] reads the line after an S3 key call, which [onDismissKeyLine] dismisses; Replace key calls [onReplaceKey].
 */
@Composable
internal fun AndroidSettingsSync(
    line: SyncLine,
    notices: @Composable ColumnScope.() -> Unit,
    location: BridgeStorageLocation?,
    access: BridgeMemberAccess?,
    keyLine: () -> S3KeyReplacement.Line?,
    onReplaceKey: () -> Unit,
    onDismissKeyLine: () -> Unit,
    restoreCode: RestoreCodeState,
    onBack: () -> Unit,
    onFix: (BridgeSyncFix) -> Unit,
    onShowRestoreCode: () -> Unit,
    onHideRestoreCode: () -> Unit,
    onPrintRestoreCode: (String) -> Unit,
) {
    AndroidScreen(title = stringResource(R.string.settings_sync), onBack = onBack, modifier = Modifier.fillMaxSize()) {
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            item { SyncStatusBlock(line = line, onFix = onFix) }
            item { Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related), content = notices) }
            location?.let { place ->
                item {
                    StorageSection(
                        location = place,
                        access = access,
                        keyLine = keyLine,
                        onReplaceKey = onReplaceKey,
                        onDismissKeyLine = onDismissKeyLine,
                    )
                }
                item {
                    RestoreCodeSectionContent(
                        needsS3Key = place is BridgeStorageLocation.S3,
                        state = restoreCode,
                        onShow = onShowRestoreCode,
                        onHide = onHideRestoreCode,
                        onPrint = onPrintRestoreCode,
                    )
                }
            }
        }
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
        AndroidNotice(
            setup.error,
            action =
                ErrorAction(stringResource(R.string.dismiss), enabled = !dismissing) {
                    dismissing = true
                    error = null
                    scope.launch {
                        performBridgeAction(
                            logger,
                            "dismiss the undone storage setup",
                            session.errors,
                            { error = it },
                        ) {
                            session.appHandle.dismissUndoneStorageSetup(setup.operation)
                        }
                        dismissing = false
                    }
                },
        )
    }
    error?.let { AndroidNotice(it) }
}

/** The dialogs a fix opens. */
@Composable
private fun SyncDialogs(
    runner: SyncFixRunner,
    actions: SessionActions,
) {
    runner.keyReplacement.sheet?.let { sheet ->
        S3KeyDialog(
            sheet = sheet,
            onSave = runner.keyReplacement::save,
            onCancel = runner.keyReplacement::cancel,
        )
    }
    runner.resetting?.let { prepared ->
        DeviceResetDialog(
            prepared = prepared,
            onConfirm = {
                runner.resetting = null
                actions.resetDevice(prepared.reset)
            },
            onDismiss = { runner.resetting = null },
        )
    }
}

/**
 * The top of the page: the state in words and the color core gives it, then its cause with any countdown under it, its
 * fix and when sync last finished, each only when core sends one. Until the status is first read it shows nothing.
 */
@Composable
internal fun SyncStatusBlock(
    line: SyncLine,
    onFix: (BridgeSyncFix) -> Unit,
    nowMs: () -> Long = System::currentTimeMillis,
) {
    val context = LocalContext.current
    when (line) {
        SyncLine.Checking -> {
            Unit
        }

        SyncLine.Unreadable -> {
            AndroidCard {
                AndroidRow(
                    title = stringResource(R.string.sync_status),
                    subtitle = stringResource(R.string.settings_sync_unreadable),
                    tone = StatusTone.DANGER,
                )
            }
        }

        is SyncLine.Read -> {
            val shown = line.shown
            val fixLabel = shown.fix?.let { syncFixLabel(it) }
            AndroidCard {
                AndroidRow(
                    title = stringResource(R.string.sync_status),
                    subtitle = syncStatusText(context, shown),
                    tone = shown.tone.statusTone(),
                    followed = shown.cause != null || fixLabel != null || shown.lastSynced != null,
                )
                shown.cause?.let { cause ->
                    Column(
                        modifier = Modifier.joinsTileAbove().padding(CAUSE_PADDING),
                        verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
                    ) {
                        Text(text = syncCauseText(context, cause), style = ThemeText.body.style)
                        (cause as? BridgeSyncCause.Message)?.countdown?.let { countdown ->
                            SyncCountdownLine(countdown, nowMs)
                        }
                    }
                }
                shown.fix?.let { fix ->
                    fixLabel?.let { label ->
                        AndroidButton(
                            onClick = { onFix(fix) },
                            modifier = Modifier.joinsTileAbove().padding(FIX_PADDING),
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
                        modifier = Modifier.joinsTileAbove().padding(LAST_SYNCED_PADDING),
                    )
                }
            }
        }
    }
}

/** The line under a cause that counts down to core's moment, redrawn each whole second and read out as it changes. */
@Composable
private fun SyncCountdownLine(
    countdown: BridgeSyncCountdown,
    nowMs: () -> Long,
) {
    Text(
        text = rememberCountdownLine(countdown.key, countdown.until.toEpochMilli(), nowMs),
        style = ThemeText.body.style,
        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
    )
}

/** The words on a fix's button: this app's own, but for the ones core words. Nothing for a fix Android never gets. */
@Composable
private fun syncFixLabel(fix: BridgeSyncFix): String? {
    val context = LocalContext.current
    return when (fix) {
        BridgeSyncFix.RetryNow -> context.coreString("core.activity.retry_now")

        BridgeSyncFix.Reconnect -> context.coreString("core.pending.action.reconnect")

        BridgeSyncFix.CheckForUpdates -> stringResource(R.string.sync_fix_update)

        is BridgeSyncFix.SignIn -> stringResource(R.string.sync_fix_sign_in, cloudProviderLabel(fix.provider))

        BridgeSyncFix.EnterAccessKey -> stringResource(R.string.sync_fix_sign_in_s3)

        BridgeSyncFix.ResetDevice -> context.coreString("core.sync.reset_from_cloud")

        BridgeSyncFix.EnterRestoreCode -> context.coreString("core.sync.enter_restore_code")

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
