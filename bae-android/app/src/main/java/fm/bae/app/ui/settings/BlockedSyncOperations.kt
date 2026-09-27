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
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeBlockedSyncOperation
import uniffi.bae_bridge.BridgeBlockedSyncOperationKind
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.BlockedSyncOperations")

/** The sync operations waiting for a person to retry them; nothing when there are none. */
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
        style = ThemeText.heading.style,
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

    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
        Text(text = blockedSyncOperationKindLabel(operation.kind))
        Text(
            text = operation.description,
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        // coven's own untranslated reason, naming what stopped the work.
        Text(
            text = operation.error,
            style = ThemeText.mono.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        retryError?.let { message ->
            Text(
                text = message,
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.error,
            )
        }
        Row(
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
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
                CircularProgressIndicator(modifier = Modifier.size(ThemeIcon.medium), strokeWidth = 2.dp)
            }
        }
    }
}

/** Hands one operation back to the sync loop and returns the error line to show, or null on success. */
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
        Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
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
