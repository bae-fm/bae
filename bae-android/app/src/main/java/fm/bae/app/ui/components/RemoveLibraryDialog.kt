package fm.bae.app.ui.components

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.BaeTheme
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeUnsentWork

/**
 * What ends a library on this device: a synced library is removed, and a
 * never-synced one, which exists only here, is deleted.
 */
internal enum class LibraryEnding {
    REMOVE,
    DELETE,
    ;

    companion object {
        /** A library that can't open is removed: whether it has a cloud copy can't be read. */
        fun of(state: BridgeLibraryState): LibraryEnding =
            when (state) {
                is BridgeLibraryState.Openable -> ofSyncs(syncs = state.storage != null)
                else -> REMOVE
            }

        fun ofSyncs(syncs: Boolean): LibraryEnding = if (syncs) REMOVE else DELETE
    }
}

/** What ending a library loses, for the person to confirm. */
internal sealed interface RemovalLoss {
    /** What this device hasn't sent, as core counted it. */
    data class Counted(
        val work: BridgeUnsentWork,
    ) : RemovalLoss

    /** A library that can't open isn't opened to count, so what it loses is unknown. */
    data object Uncounted : RemovalLoss
}

/**
 * The confirmation for ending a library: what it loses, then what ending
 * does. Cancel changes nothing.
 */
@Composable
internal fun RemoveLibraryDialog(
    ending: LibraryEnding,
    loss: RemovalLoss,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val message =
        stringResource(
            when (ending) {
                LibraryEnding.REMOVE -> R.string.settings_remove_library_confirm_body
                LibraryEnding.DELETE -> R.string.library_delete_confirm_body
            },
        )
    val body =
        when (loss) {
            RemovalLoss.Uncounted -> {
                stringResource(R.string.removal_uncounted_body)
            }

            is RemovalLoss.Counted -> {
                val lost =
                    listOfNotNull(
                        loss.work.writes.takeIf { it > 0u }?.let {
                            context.coreString("core.library.removal.unsent_writes", mapOf("count" to it.toLong()))
                        },
                        loss.work.files.takeIf { it > 0u }?.let {
                            context.coreString("core.library.removal.unsent_files", mapOf("count" to it.toLong()))
                        },
                    )
                val consequence =
                    if (lost.isEmpty()) emptyList() else listOf(stringResource(R.string.unsent_edits_body))
                (lost + consequence + message).joinToString(" ")
            }
        }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(
                stringResource(
                    when (ending) {
                        LibraryEnding.REMOVE -> R.string.settings_remove_library_confirm_title
                        LibraryEnding.DELETE -> R.string.library_delete_confirm_title
                    },
                ),
            )
        },
        text = { Text(body) },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text(
                    stringResource(
                        when (ending) {
                            LibraryEnding.REMOVE -> R.string.settings_remove
                            LibraryEnding.DELETE -> R.string.library_delete
                        },
                    ),
                )
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

@Preview(showBackground = true)
@Composable
private fun RemoveLibraryDialogPreview() {
    BaeTheme {
        RemoveLibraryDialog(
            ending = LibraryEnding.REMOVE,
            loss = RemovalLoss.Counted(BridgeUnsentWork(writes = 3u, files = 0u)),
            onConfirm = {},
            onDismiss = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun DeleteLibraryDialogPreview() {
    BaeTheme {
        RemoveLibraryDialog(
            ending = LibraryEnding.DELETE,
            loss = RemovalLoss.Counted(BridgeUnsentWork(writes = 0u, files = 0u)),
            onConfirm = {},
            onDismiss = {},
        )
    }
}
