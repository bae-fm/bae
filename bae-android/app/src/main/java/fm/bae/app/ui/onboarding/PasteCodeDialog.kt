package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.BaeAlertDialog
import fm.bae.app.ui.components.NeutralTextButton

/** The lines a code field is tall at least, and the most it grows to before it scrolls. */
private const val CODE_FIELD_LINES = 3
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
    BaeAlertDialog(
        onDismissRequest = onDismiss,
        title = text.title,
        message = text.instructions,
        content = {
            OutlinedTextField(
                value = pasteInput,
                onValueChange = onInputChange,
                label = { Text(text.label) },
                modifier = Modifier.fillMaxWidth(),
                textStyle = ThemeText.mono.style,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
                minLines = CODE_FIELD_LINES,
                maxLines = CODE_FIELD_MAX_LINES,
            )
        },
        confirmButton = {
            TextButton(onClick = { onConfirm(pasteInput.trim()) }, enabled = pasteInput.trim().isNotEmpty()) {
                Text(text.confirmLabel)
            }
        },
        dismissButton = {
            NeutralTextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
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
