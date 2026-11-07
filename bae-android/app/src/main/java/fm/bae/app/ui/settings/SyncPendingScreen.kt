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
import androidx.compose.runtime.Composable
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
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgePendingAction
import uniffi.bae_bridge.BridgePendingReason
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgeSaveSyncConfig
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup

private val logger = BaeLogger("bae.SyncPendingScreen")

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

    fun perform(
        operation: String,
        action: suspend () -> Unit,
    ) {
        error = null
        scope.launch {
            performBridgeAction(logger, operation, session.errors, { error = it }, action)
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
 * Everything sync waits on, in plain words, each with its action where the
 * person can act: sign in, retry, resume uploads, restore with the person's
 * code. A wait the person can't help offers nothing; its diagnostic details
 * stay behind Details.
 */
@Composable
internal fun SyncPendingScreen(
    session: OpenLibrary,
    actions: SessionActions,
    onBack: () -> Unit,
) {
    val pending by session.syncStatusStore.pending.collectAsState()
    val line by session.syncStatusStore.line.collectAsState()
    val failure by session.syncStatusStore.error.collectAsState()
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val runner = remember(session) { PendingActionRunner(session, actions, context, scope) }

    Column(modifier = Modifier.fillMaxSize()) {
        SettingsTopBar(title = stringResource(R.string.settings_sync), onBack = onBack)
        syncLineText(context, line)?.let { text ->
            Text(
                text = text,
                style = ThemeText.heading.style,
                modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
            )
        }
        Column(
            modifier = Modifier.padding(horizontal = ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            SyncFailureNotice(
                error = failure,
                onReconnect = { runner.perform("reconnect sync") { session.appHandle.reconnectSync() } },
            )
            DeviceResetOffer(session = session, error = failure, onResetDevice = actions.resetDevice)
            runner.signIns.steps?.let { StorageSetupLine(it) }
            runner.error?.let { ErrorDetailDisclosure(it) }
        }
        // What is waiting is a local read; nothing shows until it lands.
        pending?.let { records -> PendingList(records = records, onAction = runner::run) }
    }
    runner.askingForKey?.let { location ->
        S3KeyDialog(
            location = location,
            onSignIn = runner::signInWithKey,
            onDismiss = { runner.askingForKey = null },
        )
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

@Composable
private fun PendingList(
    records: List<BridgePendingRecord>,
    onAction: (BridgePendingAction) -> Unit,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        itemsIndexed(records) { index, record ->
            if (index > 0) HorizontalDivider()
            PendingRow(record = record, onAction = onAction)
        }
    }
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
private fun PendingListPreview() {
    BaeTheme {
        PendingList(
            records =
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
                ),
            onAction = {},
        )
    }
}
