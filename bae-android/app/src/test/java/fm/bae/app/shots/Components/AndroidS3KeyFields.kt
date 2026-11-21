package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidS3KeyFields
import org.junit.Test

/** The component shots of `AndroidS3KeyFields`, one for each row of its preview. */
class AndroidS3KeyFieldsShots : BoardShot() {
    @Test
    fun androidComponentS3KeyFieldsEmpty() = component("android.component.s3-key-fields.empty") { KeyFields("", "") }

    @Test
    fun androidComponentS3KeyFieldsTyped() = component("android.component.s3-key-fields.typed") { KeyFields(KEY_ID, SECRET) }

    @Test
    fun androidComponentS3KeyFieldsIdFocused() =
        component("android.component.s3-key-fields.id-focused") {
            val focus = remember { FocusRequester() }
            KeyFields(KEY_ID, SECRET, idModifier = Modifier.focusRequester(focus))
            LaunchedEffect(Unit) { focus.requestFocus() }
        }

    @Test
    fun androidComponentS3KeyFieldsRefused() =
        component("android.component.s3-key-fields.refused") {
            KeyFields(KEY_ID, SECRET, note = "S3 refused this key.")
        }
}

private const val KEY_ID = "AKIA4EXAMPLENEW"

private const val SECRET = "abcdefghijkl"

@Composable
private fun KeyFields(
    id: String,
    secret: String,
    idModifier: Modifier = Modifier,
    note: String? = null,
) = Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.group)) {
    AndroidS3KeyFields(
        accessKeyId = id,
        secretAccessKey = secret,
        onAccessKeyIdChange = {},
        onSecretAccessKeyChange = {},
        secretNote = note,
        accessKeyIdModifier = idModifier,
    )
}
