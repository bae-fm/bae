package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.title
import fm.bae.app.ui.appearance.ThemeSpace
import uniffi.bae_bridge.BridgeLibrary

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
    AndroidDialog(onDismissRequest = onDismiss, parts = removeLibraryParts(library, loss, onConfirm, onDismiss))
}

/**
 * What the confirmation for ending [library] says and offers: the title naming it, then the warning when
 * its [loss] is unsent changes, then what ending it does, with Cancel and the ending's button.
 */
@Composable
internal fun removeLibraryParts(
    library: BridgeLibrary,
    loss: RemovalLoss,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts {
    val words = removalWords(library, loss)
    return DialogParts(
        title = words.title,
        message = words.lead,
        content = words.rest,
        confirmButton = {
            // Removing loses nothing, so it is the accent action.
            AndroidButton(
                kind = ButtonKind.TEXT,
                onClick = onConfirm,
            ) { Text(stringResource(R.string.settings_remove)) }
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
    val title = stringResource(R.string.library_remove_confirm_title, library.title())
    val message = stringResource(R.string.settings_remove_library_confirm_body)
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
                        AndroidNotice(
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
