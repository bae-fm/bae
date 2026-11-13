package fm.bae.app.ui.onboarding

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import uniffi.bae_bridge.BridgeLibrary

/**
 * The confirmation to discard an unfinished join this phone can't read: it
 * leaves this phone, and the library in the cloud stays as it is. Cancel
 * changes nothing.
 */
@Composable
internal fun DiscardInstallationDialog(
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(onDismissRequest = onDismiss, parts = discardInstallationParts(onConfirm, onDismiss))
}

/** What the confirmation to discard an unreadable unfinished join says and offers. */
@Composable
private fun discardInstallationParts(
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts =
    DialogParts(
        title = stringResource(R.string.onboarding_discard_join_title),
        message = stringResource(R.string.onboarding_discard_join_message),
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.DESTRUCTIVE, onClick = onConfirm) {
                Text(stringResource(R.string.onboarding_discard_join_confirm))
            }
        },
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )

/**
 * The welcome's list of this device's libraries over [model], which opens one
 * with [onOpen], and lists [flow]'s unreadable unfinished join among them, to
 * discard after the person confirms.
 */
internal fun welcomeLibrariesView(
    model: WelcomeLibraries,
    flow: OnboardingFlow,
    onOpen: (BridgeLibrary) -> Unit,
): WelcomeLibrariesView {
    val listed =
        (model.list as? WelcomeLibrariesList.Loaded)?.libraries?.isNotEmpty() == true ||
            flow.unreadable.libraryId != null
    return WelcomeLibrariesView(listed = listed) { onAdd ->
        WelcomeLibrariesSection(
            model,
            onOpen = onOpen,
            onAdd = onAdd,
            unreadableInstallation = flow.unreadable.libraryId,
            onDiscardInstallation = flow.unreadable::ask,
        )
        flow.unreadable.asking?.let { libraryId ->
            DiscardInstallationDialog(
                onConfirm = { flow.unreadable.discard(libraryId) },
                onDismiss = flow.unreadable::dismiss,
            )
        }
    }
}
