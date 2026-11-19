package fm.bae.app.ui.onboarding

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidTextField
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.TextFieldKind

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
        action =
            DialogAction(
                text.confirmLabel,
                { onConfirm(pasteInput.trim()) },
                enabled = pasteInput.trim().isNotEmpty(),
            ),
        dismiss = DialogAction(stringResource(R.string.cancel), onDismiss),
    )
}
