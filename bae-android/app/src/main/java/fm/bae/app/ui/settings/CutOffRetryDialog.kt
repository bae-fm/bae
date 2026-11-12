package fm.bae.app.ui.settings

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import uniffi.bae_bridge.BridgeDeviceCutOff

/** The cut-off confirmation changed nothing: a request to the provider ran, or the removal hasn't arrived yet. */
@Composable
internal fun CutOffRetryDialog(
    reason: BridgeDeviceCutOff,
    onTryAgain: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        parts = cutOffRetryParts(reason = reason, onTryAgain = onTryAgain, onDismiss = onDismiss),
    )
}

@Composable
internal fun cutOffRetryParts(
    reason: BridgeDeviceCutOff,
    onTryAgain: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts {
    val notGone = reason == BridgeDeviceCutOff.DEVICE_NOT_GONE
    return DialogParts(
        title =
            stringResource(
                if (notGone) R.string.device_cut_off_not_gone_title else R.string.device_cut_off_running_title,
            ),
        message =
            stringResource(
                if (notGone) R.string.device_cut_off_not_gone_body else R.string.device_cut_off_running_body,
            ),
        confirmButton = {
            AndroidButton(
                kind = ButtonKind.TEXT,
                onClick = onTryAgain,
            ) { Text(stringResource(R.string.retry)) }
        },
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDismiss) {
                Text(stringResource(R.string.device_cut_off_not_yet))
            }
        },
    )
}
