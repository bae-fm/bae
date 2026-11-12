package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone

/** A code field is tall at least as three 24dp lines and the design's 28dp of padding, and grows to six lines. */
private val CODE_FIELD_MIN_HEIGHT = 98.dp
private const val CODE_FIELD_MAX_LINES = 6

/** The fixed wording of a [PasteCodeDialog], distinct per code it accepts. */
class PasteDialogText(
    val title: String,
    val instructions: String,
    val label: String,
    val confirmLabel: String,
)

@Composable
fun PasteCodeDialog(
    text: PasteDialogText,
    pasteInput: String,
    onInputChange: (String) -> Unit,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        title = text.title,
        message = text.instructions,
        content = {
            OutlinedTextField(
                value = pasteInput,
                onValueChange = onInputChange,
                label = { Text(text.label) },
                modifier = Modifier.fillMaxWidth().heightIn(min = CODE_FIELD_MIN_HEIGHT),
                textStyle = ThemeText.mono.style,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
                maxLines = CODE_FIELD_MAX_LINES,
            )
        },
        confirmButton = {
            AndroidButton(
                kind = ButtonKind.TEXT,
                onClick = { onConfirm(pasteInput.trim()) },
                enabled = pasteInput.trim().isNotEmpty(),
            ) {
                Text(text.confirmLabel)
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

@Preview
@Composable
private fun PasteCodeDialogPreview() {
    BaeTheme {
        PasteCodeDialog(
            text =
                PasteDialogText(
                    title = "Paste code",
                    instructions = "Paste the code from your other device.",
                    label = "Code",
                    confirmLabel = "Connect",
                ),
            pasteInput = "placeholder-code",
            onInputChange = {},
            onConfirm = {},
            onDismiss = {},
        )
    }
}
