package fm.bae.app.ui.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
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
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        placeholder = placeholder?.let { text -> { Text(text = text, modifier = Modifier.clearAndSetSemantics {}) } },
        supportingText = helper?.let { line -> { Text(line) } },
        isError = error,
        enabled = enabled,
        modifier = if (code) modifier.fillMaxWidth().heightIn(min = CODE_FIELD_MIN_HEIGHT) else modifier.fillMaxWidth(),
        textStyle = if (code) ThemeText.mono.style else LocalTextStyle.current,
        visualTransformation =
            when (kind) {
                TextFieldKind.PLAIN -> VisualTransformation.None
                TextFieldKind.CODE -> WordGapTransformation
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
}

/** A code field is tall at least as three 24dp lines and the design's 28dp of padding, and grows to six lines. */
private val CODE_FIELD_MIN_HEIGHT = 98.dp
private const val CODE_FIELD_MAX_LINES = 6
