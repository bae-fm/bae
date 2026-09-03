package fm.bae.app.ui.settings

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.R
import fm.bae.app.localizedLine
import fm.bae.app.ui.BaeTheme
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeBlockedSyncOperation
import uniffi.bae_bridge.BridgeBlockedSyncOperationKind
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.BlockedSyncOperations")

/**
 * The durable sync operations a completed cycle left waiting on a person. Each
 * failed on a fault that running it again cannot change, so later cycles skip it
 * and it moves only when someone taps Retry; a row leaves the list when the next
 * sync status no longer names it. Renders nothing while there are none.
 */
@Composable
internal fun BlockedSyncOperations(
    operations: List<BridgeBlockedSyncOperation>,
    onRetry: suspend (String) -> Unit,
) {
    if (operations.isEmpty()) {
        return
    }
    Text(
        text = stringResource(R.string.settings_sync_waiting),
        style = MaterialTheme.typography.titleSmall,
    )
    operations.forEach { operation ->
        BlockedSyncOperationRow(operation = operation, onRetry = onRetry)
    }
}

@Composable
private fun BlockedSyncOperationRow(
    operation: BridgeBlockedSyncOperation,
    onRetry: suspend (String) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var retrying by remember(operation.id) { mutableStateOf(false) }
    var retryError by remember(operation.id) { mutableStateOf<String?>(null) }

    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(text = blockedSyncOperationKindLabel(operation.kind))
        Text(
            text = operation.description,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        // coven's own reason, untranslated. The kind above names the work in the
        // reader's language; this names what stopped it, which is the part they
        // can act on or paste into a report.
        Text(
            text = operation.error,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        retryError?.let { message ->
            Text(
                text = message,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
            )
        }
        Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedButton(
                onClick = {
                    retryError = null
                    retrying = true
                    scope.launch {
                        retryError =
                            runRetry(operation.id, onRetry, context) { retrying = false }
                    }
                },
                enabled = !retrying,
            ) {
                Text(stringResource(R.string.settings_retry))
            }
            if (retrying) {
                CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
            }
        }
    }
}

/**
 * Hand one operation back to the sync loop, returning a user-facing error line to
 * display (or null on success). A retry that takes drops the row on the next sync
 * status; one refused — the operation is no longer blocked, or the loop is not
 * running — reports here rather than leaving the button looking inert.
 */
private suspend fun runRetry(
    id: String,
    onRetry: suspend (String) -> Unit,
    context: Context,
    onSettled: () -> Unit,
): String? =
    try {
        onRetry(id)
        null
    } catch (e: CancellationException) {
        throw e
    } catch (e: BridgeException) {
        logger.error("Retrying blocked sync operation $id failed", e)
        context.localizedLine(e)
    } catch (e: Exception) {
        logger.error("Retrying blocked sync operation $id failed", e)
        e.message ?: e::class.java.simpleName
    } finally {
        onSettled()
    }

@Composable
private fun blockedSyncOperationKindLabel(kind: BridgeBlockedSyncOperationKind): String =
    when (kind) {
        BridgeBlockedSyncOperationKind.WRITE -> stringResource(R.string.sync_blocked_write)
        BridgeBlockedSyncOperationKind.CIRCLE_OPERATION -> stringResource(R.string.sync_blocked_circle_operation)
        BridgeBlockedSyncOperationKind.RECLAIM -> stringResource(R.string.sync_blocked_reclaim)
    }

@Preview(showBackground = true)
@Composable
private fun BlockedSyncOperationsPreview() {
    BaeTheme {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            BlockedSyncOperations(
                operations =
                    listOf(
                        BridgeBlockedSyncOperation(
                            id = "write:write-1",
                            kind = BridgeBlockedSyncOperationKind.WRITE,
                            description = "releases/release-3",
                            error = "blob release_files/file-7 is missing",
                        ),
                        BridgeBlockedSyncOperation(
                            id = "reclaim:9f2c",
                            kind = BridgeBlockedSyncOperationKind.RECLAIM,
                            description = "a published batch of library changes",
                            error =
                                "object store-v1/library/packages/12.json: the slot already holds another object",
                        ),
                    ),
                onRetry = {},
            )
        }
    }
}
