package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsPropertyReceiver
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.error
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/** What an [AndroidTextField] holds. */
internal enum class TextFieldKind {
    /** One line of text. */
    PLAIN,

    /** A restore code or an invite, several lines tall and monospaced, in groups of four. */
    CODE,

    /** A password or a secret key, shown as dots. */
    SECRET,
}

/**
 * An outlined text field as wide as its place, named by [label]: the [placeholder] shows in it while it
 * is empty, and the [helper] line under it, in the error colour when [error]. TalkBack reads the label as
 * the field's name and skips the placeholder. A [TextFieldKind.CODE] or [TextFieldKind.SECRET] field sets
 * its own keyboard.
 */
@Composable
internal fun AndroidTextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    kind: TextFieldKind = TextFieldKind.PLAIN,
    placeholder: String? = null,
    helper: String? = null,
    error: Boolean = false,
    enabled: Boolean = true,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
) {
    val code = kind == TextFieldKind.CODE
    // Material's field is as tall as its outline plus the 8 dp its label rises above it, and the board's field box is
    // those same two together. The helper is a text of its own under it, so TalkBack gets it through the field.
    Column {
        OutlinedTextField(
            value = value,
            onValueChange = onValueChange,
            label = { Text(label) },
            placeholder =
                placeholder?.let { text -> { Text(text = text, modifier = Modifier.clearAndSetSemantics {}) } },
            isError = error,
            enabled = enabled,
            modifier =
                (if (code) modifier.fillMaxWidth().heightIn(min = CODE_FIELD_MIN_HEIGHT) else modifier.fillMaxWidth())
                    .semantics { helperSemantics(helper, error) },
            textStyle = if (code) ThemeText.mono.style else MaterialTheme.typography.bodyLarge,
            visualTransformation =
                when (kind) {
                    TextFieldKind.PLAIN -> VisualTransformation.None
                    TextFieldKind.CODE -> CodeGroupTransformation
                    TextFieldKind.SECRET -> PasswordVisualTransformation()
                },
            keyboardOptions =
                when (kind) {
                    TextFieldKind.PLAIN -> {
                        keyboardOptions
                    }

                    TextFieldKind.CODE -> {
                        KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false)
                    }

                    TextFieldKind.SECRET -> {
                        KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false)
                    }
                },
            keyboardActions = keyboardActions,
            singleLine = !code,
            maxLines = if (code) CODE_FIELD_MAX_LINES else 1,
        )
        helper?.let { line ->
            Text(
                text = line,
                style = MaterialTheme.typography.bodySmall,
                color = if (error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = ThemeSpace.edge, top = ThemeSpace.inline),
            )
        }
    }
}

/**
 * What TalkBack reads with the field, since the helper is a text of its own: the error through `error` when the
 * field is in error, else the helper as the field's state description.
 */
private fun SemanticsPropertyReceiver.helperSemantics(
    helper: String?,
    error: Boolean,
) {
    if (helper == null) return
    if (error) error(helper) else stateDescription = helper
}

/**
 * A code field is tall at least as three 24dp lines, the design's 28dp of padding and the 8dp its label rises above
 * the outline, and grows to six lines.
 */
private val CODE_FIELD_MIN_HEIGHT = 108.dp
private const val CODE_FIELD_MAX_LINES = 6
