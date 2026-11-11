package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeStorageLocation

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
    /** Whether this device holds changes it hasn't sent, as core found it. */
    data class Counted(
        val holdsUnsent: Boolean,
    ) : RemovalLoss

    /** A library that can't open isn't opened to count, so what it loses is unknown. */
    data object Uncounted : RemovalLoss
}

/**
 * The confirmation for ending [library]: what ending it does, led by a warning
 * when this device holds changes it hasn't sent. Cancel changes nothing.
 */
@Composable
internal fun RemoveLibraryDialog(
    library: BridgeLibrary,
    loss: RemovalLoss,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val words = removalWords(library, loss)
    AndroidDialog(
        onDismissRequest = onDismiss,
        title = words.title,
        message = words.lead,
        content = words.rest,
        confirmButton = {
            // Removing loses nothing, so it is the accent action; only Delete is red.
            when (LibraryEnding.of(library.state)) {
                LibraryEnding.REMOVE -> {
                    AndroidButton(
                        kind = ButtonKind.TEXT,
                        onClick = onConfirm,
                    ) { Text(stringResource(R.string.settings_remove)) }
                }

                LibraryEnding.DELETE -> {
                    AndroidButton(
                        kind = ButtonKind.TEXT,
                        tone = ButtonTone.DESTRUCTIVE,
                        onClick = onConfirm,
                    ) { Text(stringResource(R.string.library_delete)) }
                }
            }
        },
        dismissButton = {
            AndroidButton(
                kind = ButtonKind.TEXT,
                tone = ButtonTone.NEUTRAL,
                onClick = onDismiss,
            ) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/**
 * The confirmation's words: the title naming [library], then the warning when
 * its [loss] is unsent changes, then what ending it does. It is separate from
 * the dialog so a screenshot can draw it: an [AlertDialog] opens a window the
 * screenshot tests don't render.
 */
@Composable
internal fun RemoveLibraryText(
    library: BridgeLibrary,
    loss: RemovalLoss,
) {
    val words = removalWords(library, loss)
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.edge)) {
        DialogHead(words.title, words.lead)
        words.rest?.invoke()
    }
}

/** The confirmation's title, the sentence set under it, and whatever follows. */
private class RemovalWords(
    val title: String,
    val lead: String?,
    val rest: @Composable (() -> Unit)?,
)

@Composable
private fun removalWords(
    library: BridgeLibrary,
    loss: RemovalLoss,
): RemovalWords {
    val ending = LibraryEnding.of(library.state)
    val title =
        stringResource(
            when (ending) {
                LibraryEnding.REMOVE -> R.string.library_remove_confirm_title
                LibraryEnding.DELETE -> R.string.library_delete_confirm_title
            },
            library.name,
        )
    val message =
        stringResource(
            when (ending) {
                LibraryEnding.REMOVE -> R.string.settings_remove_library_confirm_body
                LibraryEnding.DELETE -> R.string.library_delete_confirm_body
            },
        )
    val bodyColor = MaterialTheme.colorScheme.onSurfaceVariant
    return when (loss) {
        RemovalLoss.Uncounted -> {
            RemovalWords(title, stringResource(R.string.removal_uncounted_body)) {
                Text(message, color = bodyColor)
            }
        }

        is RemovalLoss.Counted -> {
            if (loss.holdsUnsent) {
                RemovalWords(title, null) {
                    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.group)) {
                        TitledErrorNotice(
                            title = stringResource(R.string.library_unsent_changes_lost),
                            advice = null,
                            error = null,
                            tone = StatusTone.WARNING,
                            look = NoticeLook.QUIET,
                        )
                        Text(message, color = bodyColor)
                    }
                }
            } else {
                RemovalWords(title, message, null)
            }
        }
    }
}

private val previewHome =
    BridgeLibrary(
        id = "home",
        name = "Home music",
        path = "/libraries/home",
        state =
            BridgeLibraryState.Openable(
                BridgeStorageLocation.Dropbox(
                    BridgeDropboxLocation.AppFolder,
                    accountLabel = "name@example.com",
                    folderLabel = null,
                ),
            ),
        isActive = false,
    )

private val previewLocal =
    previewHome.copy(id = "local", name = "Local music", state = BridgeLibraryState.Openable(storage = null))

private val previewUnreadable =
    previewHome.copy(id = "stopped", name = "Half removed", state = BridgeLibraryState.DeletionUnfinished)

private val nothingUnsent = RemovalLoss.Counted(holdsUnsent = false)

private val someUnsent = RemovalLoss.Counted(holdsUnsent = true)

@Preview(showBackground = true)
@Composable
private fun RemoveLibraryUnsentPreview() {
    BaeTheme { RemoveLibraryText(previewHome, someUnsent) }
}

@Preview(showBackground = true)
@Composable
private fun RemoveLibraryPreview() {
    BaeTheme { RemoveLibraryText(previewHome, nothingUnsent) }
}

@Preview(showBackground = true)
@Composable
private fun DeleteLibraryPreview() {
    BaeTheme { RemoveLibraryText(previewLocal, nothingUnsent) }
}

@Preview(showBackground = true)
@Composable
private fun RemoveUnreadableLibraryPreview() {
    BaeTheme { RemoveLibraryText(previewUnreadable, RemovalLoss.Uncounted) }
}
