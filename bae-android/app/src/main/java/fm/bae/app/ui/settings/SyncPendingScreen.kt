package fm.bae.app.ui.settings

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.LocaleErrorLines
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.deviceName
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeAdminAccess
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeOperationKind
import uniffi.bae_bridge.BridgePendingAction
import uniffi.bae_bridge.BridgePendingReason
import uniffi.bae_bridge.BridgePendingReasonKind
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgePendingRecords
import uniffi.bae_bridge.BridgePendingSubject
import uniffi.bae_bridge.bridgePendingActionKey
import uniffi.bae_bridge.bridgePendingReasonKey
import uniffi.bae_bridge.bridgePendingSubjectKey

private val logger = BaeLogger("bae.SyncPendingScreen")

/** The access-key deletion a pending record asks for, while the person chooses how. */
private data class KeyDeletion(
    val operation: Long,
    val accessKeyId: String,
)

/**
 * Runs the action a pending record offers. Most run at once; an
 * administrator's credentials and an access key's deletion first ask the
 * person through a dialog, which [PendingActionDialogs] shows. Restoring with
 * the person's code closes the library for the restore screen.
 */
private class PendingActionRunner(
    private val session: OpenLibrary,
    private val actions: SessionActions,
    private val context: Context,
    private val scope: CoroutineScope,
) {
    var error by mutableStateOf<String?>(null)
    var adminAccessFor by mutableStateOf<Long?>(null)
    var keyDeletion by mutableStateOf<KeyDeletion?>(null)

    fun perform(
        operation: String,
        action: suspend () -> Unit,
    ) {
        error = null
        scope.launch {
            performBridgeAction(logger, operation, LocaleErrorLines(context), { error = it }, action)
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

            BridgePendingAction.ResumeUploads -> {
                perform("resume uploads") { handle.setSyncPaused(false) }
            }

            is BridgePendingAction.RetryOperation -> {
                perform("retry pending operation ${action.id}") { handle.retryPendingOperation(action.id) }
            }

            BridgePendingAction.RestoreFromCode -> {
                actions.restoreFromCode()
            }

            is BridgePendingAction.ProvideAdminAccess -> {
                adminAccessFor = action.id
            }

            is BridgePendingAction.DeleteAccessKey -> {
                keyDeletion = KeyDeletion(action.operation, action.accessKeyId)
            }
        }
    }

    private fun signIn(provider: BridgeCloudProvider) {
        val providers = actions.oauthProviders
        if (providers == null) {
            error = context.getString(R.string.oauth_unconfigured)
        } else {
            perform("sign in to $provider again") { providers.signInAgain(session.appHandle, deviceName(context)) }
        }
    }

    fun retryWithAdminAccess(
        operation: Long,
        access: BridgeAdminAccess,
    ) {
        adminAccessFor = null
        perform("retry operation $operation with administrator access") {
            session.appHandle.retryWithAdminAccess(operation, access)
        }
    }

    fun deleteKey(
        deletion: KeyDeletion,
        access: BridgeAdminAccess,
    ) {
        keyDeletion = null
        perform("delete access key ${deletion.accessKeyId}") {
            session.appHandle.deleteAccessKey(deletion.operation, access)
        }
    }

    fun confirmKeyDeleted(deletion: KeyDeletion) {
        keyDeletion = null
        perform("confirm access key ${deletion.accessKeyId} deleted") {
            session.appHandle.confirmAccessKeyDeleted(deletion.accessKeyId)
        }
    }
}

/**
 * Everything sync waits on, in plain words, each with its action where the
 * person can act: sign in, retry, resume uploads, restore with the person's
 * code, give administrator access. A wait the person can't help offers nothing; its
 * diagnostic details stay behind Details.
 */
@Composable
internal fun SyncPendingScreen(
    session: OpenLibrary,
    actions: SessionActions,
    onBack: () -> Unit,
) {
    val pending by session.syncStatusStore.pending.collectAsState()
    val indicator by session.syncStatusStore.indicator.collectAsState()
    val failure by session.syncStatusStore.error.collectAsState()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val runner = remember(session) { PendingActionRunner(session, actions, context, scope) }

    Column(modifier = Modifier.fillMaxSize()) {
        PendingTopBar(onBack = onBack)
        Text(
            text = syncIndicatorText(context, indicator),
            style = ThemeText.heading.style,
            modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        )
        Column(
            modifier = Modifier.padding(horizontal = ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            SyncFailureNotice(
                indicator = indicator,
                error = failure,
                onReconnect = { runner.perform("reconnect sync") { session.appHandle.reconnectSync() } },
            )
            DeviceResetOffer(session = session, error = failure, onResetDevice = actions.resetDevice)
            runner.error?.let { ErrorText(it) }
        }
        when (val current = pending) {
            BridgePendingRecords.Loading -> {
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator()
                }
            }

            is BridgePendingRecords.Failed -> {
                ErrorText(
                    LocaleErrorLines(context).line(current.error) ?: current.error.toString(),
                    modifier = Modifier.padding(ThemeSpace.edge),
                )
            }

            is BridgePendingRecords.Ready -> {
                PendingList(records = current.records, onAction = runner::run)
            }
        }
    }
    PendingActionDialogs(runner)
}

/** The dialog an action waits on, while it waits. */
@Composable
private fun PendingActionDialogs(runner: PendingActionRunner) {
    runner.adminAccessFor?.let { operation ->
        AdminAccessDialog(
            title = stringResource(R.string.admin_access_title),
            onSubmit = { runner.retryWithAdminAccess(operation, it) },
            onDismiss = { runner.adminAccessFor = null },
        )
    }
    runner.keyDeletion?.let { deletion ->
        DeleteAccessKeyDialog(
            accessKeyId = deletion.accessKeyId,
            onDeleteWith = { runner.deleteKey(deletion, it) },
            onDeletedElsewhere = { runner.confirmKeyDeleted(deletion) },
            onDismiss = { runner.keyDeletion = null },
        )
    }
}

/** Open bae's page in the device's app store; the line to show when no store can open it. */
private fun openStoreListing(context: Context): String? =
    try {
        context.startActivity(
            Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=${context.packageName}"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        null
    } catch (e: ActivityNotFoundException) {
        logger.warning("no app store can open bae's listing", e)
        context.getString(R.string.pending_update_no_store)
    }

@Composable
private fun PendingTopBar(onBack: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
            }
            Text(text = stringResource(R.string.settings_sync), style = ThemeText.title.style)
        }
    }
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

/** What a pending record is about: a file by its name and album, anything else by what it is. */
internal fun pendingSubjectText(
    context: Context,
    subject: BridgePendingSubject,
): String {
    val key = bridgePendingSubjectKey(subject)
    return when (subject) {
        is BridgePendingSubject.ReleaseFile -> {
            context.coreString(key, mapOf("file" to subject.fileName, "album" to subject.albumTitle))
        }

        is BridgePendingSubject.Cover -> {
            context.coreString(key, mapOf("album" to subject.albumTitle))
        }

        else -> {
            context.coreString(key)
        }
    }
}

/** One record: what it is about, why it waits, what its causes were, its action, and its details on request. */
@Composable
private fun PendingRow(
    record: BridgePendingRecord,
    onAction: (BridgePendingAction) -> Unit,
) {
    val context = LocalContext.current
    var showDetails by remember(record) { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
        Text(text = pendingSubjectText(context, record.subject), style = ThemeText.strong.style)
        Text(text = context.coreString(bridgePendingReasonKey(record.reason.kind)))
        record.reason.causes.forEach { cause ->
            Text(
                text = context.coreString(bridgePendingReasonKey(cause.kind)),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Row(
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val actionKey = bridgePendingActionKey(record.action)
            when (record.action) {
                // Details below show what a review is of.
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
            TextButton(onClick = { showDetails = !showDetails }) {
                Text(stringResource(R.string.details))
            }
        }
        if (showDetails) {
            SelectionContainer {
                Text(
                    text = record.reason.detail,
                    style = ThemeText.mono.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
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
                        subject = BridgePendingSubject.Connection,
                        reason =
                            BridgePendingReason(
                                kind = BridgePendingReasonKind.PROVIDER_PENDING,
                                detail = "ProviderPending",
                                causes = emptyList(),
                            ),
                        action = BridgePendingAction.SignIn(BridgeCloudProvider.DROPBOX),
                    ),
                    BridgePendingRecord(
                        subject = BridgePendingSubject.Operation(BridgeOperationKind.INVITE),
                        reason = BridgePendingReason(BridgePendingReasonKind.FAILED, "Failed(Storage)", emptyList()),
                        action = BridgePendingAction.RetryOperation(id = 7L),
                    ),
                ),
            onAction = {},
        )
    }
}
