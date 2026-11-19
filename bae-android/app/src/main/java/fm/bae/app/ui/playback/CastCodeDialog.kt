package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.CastStart
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidPinField
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.AndroidTextField
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.TextFieldKind
import fm.bae.app.ui.components.focusOnShow
import uniffi.bae_bridge.BridgeAirPlayCode
import uniffi.bae_bridge.BridgeAirPlayCodeRequest
import uniffi.bae_bridge.bridgeAirplayCodeFieldKey
import uniffi.bae_bridge.bridgeAirplayCodeMessageKey

/** The pairing dialog's width, as the boards draw it. */
private val DIALOG_WIDTH = 340.dp

/**
 * The dialog that asks for the PIN or password an AirPlay receiver wants while a cast to it connects, over
 * whatever is on screen. The person's code goes to [onPair]; Cancel, Back and a tap outside go to [onCancel].
 */
@Composable
internal fun CastCodeDialog(
    sheet: CastStart.CodeSheet,
    onPair: (String) -> Unit,
    onCancel: () -> Unit,
) {
    val context = LocalContext.current
    val receiver = sheet.receiver.name
    // The typed code stays through a wrong code, and core's line about the code it refused shows until the person
    // edits it. Kept in memory only: a password never goes into saved state.
    var code by remember(sheet.attempt) { mutableStateOf("") }
    // Core asks again once the code it was sent fails.
    var editedSinceAsked by remember(sheet.attempt, sheet.pairing) { mutableStateOf(false) }
    val fieldLine =
        if (editedSinceAsked || sheet.pairing) {
            null
        } else {
            bridgeAirplayCodeFieldKey(sheet.request)?.let { key ->
                // Only a PIN's line counts its digits.
                context.coreString(key, sheet.request.digits?.let { mapOf("digits" to it) } ?: emptyMap())
            }
        }
    AndroidDialog(
        onDismissRequest = onCancel,
        parts =
            castCodeDialogParts(
                receiver = receiver,
                message = context.coreString(bridgeAirplayCodeMessageKey(sheet.request), mapOf("receiver" to receiver)),
                request = sheet.request,
                pairing = sheet.pairing,
                code = code,
                fieldLine = fieldLine,
                onCodeChange = {
                    code = it
                    editedSinceAsked = true
                },
                onPair = { onPair(code) },
                onCancel = onCancel,
            ),
    )
}

/** How many digits the PIN has, or null for a password. */
internal val BridgeAirPlayCodeRequest.digits: Int?
    get() = (code as? BridgeAirPlayCode.Pin)?.digits?.toInt()

/**
 * The pairing dialog's parts: titled with the [receiver], core's [message] under the title, the PIN's boxes or a
 * password field holding [code] with [fieldLine] under it, and Pairing… with the field and Pair off while [pairing].
 * Pair is on once the PIN has all its digits, or the password has any text.
 */
@Composable
internal fun castCodeDialogParts(
    receiver: String,
    message: String,
    request: BridgeAirPlayCodeRequest,
    pairing: Boolean,
    code: String,
    fieldLine: String?,
    onCodeChange: (String) -> Unit,
    onPair: () -> Unit,
    onCancel: () -> Unit,
): DialogParts {
    val digits = request.digits
    val full = if (digits != null) code.length == digits else code.isNotEmpty()
    val canPair = !pairing && full
    return DialogParts(
        title = stringResource(R.string.cast_pair_with, receiver),
        message = message,
        width = DIALOG_WIDTH,
        content = {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                // The field takes the keyboard as the dialog opens.
                val focus = if (!pairing) Modifier.focusOnShow() else Modifier
                if (digits != null) {
                    AndroidPinField(
                        digits = digits,
                        value = code,
                        onValueChange = onCodeChange,
                        modifier = focus,
                        error = fieldLine,
                        enabled = !pairing,
                    )
                } else {
                    AndroidTextField(
                        value = code,
                        onValueChange = onCodeChange,
                        label = stringResource(R.string.cast_password),
                        modifier = focus,
                        kind = TextFieldKind.SECRET,
                        helper = fieldLine,
                        error = fieldLine != null,
                        enabled = !pairing,
                    )
                }
                if (pairing) {
                    AndroidSpinner(stringResource(R.string.cast_pairing))
                }
            }
        },
        action = DialogAction(stringResource(R.string.cast_pair), onPair, enabled = canPair),
        dismiss = DialogAction(stringResource(R.string.cancel), onCancel),
    )
}
