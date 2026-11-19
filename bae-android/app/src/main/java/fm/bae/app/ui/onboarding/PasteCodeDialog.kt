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
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidTextField
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.TextFieldKind
import fm.bae.app.ui.components.WordGapTransformation

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
            AndroidTextField(
                value = pasteInput,
                onValueChange = onInputChange,
                label = text.label,
                kind = TextFieldKind.CODE,
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
