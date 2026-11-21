package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import fm.bae.app.ui.components.AndroidTextField
import fm.bae.app.ui.components.TextFieldKind
import org.junit.Test

/** The component shots of `AndroidTextField`, one for each row of its preview. */
class AndroidTextFieldShots : BoardShot() {
    @Test
    fun androidComponentTextFieldResting() =
        component("android.component.text-field.resting", wide = true) { NameField(value = "Home music") }

    @Test
    fun androidComponentTextFieldFocused() =
        component("android.component.text-field.focused", wide = true) {
            val focus = remember { FocusRequester() }
            NameField(value = "Home music", modifier = Modifier.focusRequester(focus))
            LaunchedEffect(Unit) { focus.requestFocus() }
        }

    @Test
    fun androidComponentTextFieldHelper() =
        component("android.component.text-field.helper", wide = true) {
            NameField(value = "Home music", helper = "People in this library see this name.")
        }

    @Test
    fun androidComponentTextFieldError() =
        component("android.component.text-field.error", wide = true) { NameField(value = "", helper = "Enter a name.", error = true) }

    @Test
    fun androidComponentTextFieldSecret() =
        component("android.component.text-field.secret", wide = true) { NameField(value = "Home music", kind = TextFieldKind.SECRET) }
}

@Composable
private fun NameField(
    value: String,
    modifier: Modifier = Modifier,
    helper: String? = null,
    error: Boolean = false,
    kind: TextFieldKind = TextFieldKind.PLAIN,
) = AndroidTextField(
    value = value,
    onValueChange = {},
    label = "Name",
    modifier = modifier,
    kind = kind,
    helper = helper,
    error = error,
)
