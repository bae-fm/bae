package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidS3KeyFields
import fm.bae.app.ui.components.focusOnShow
import org.junit.Test

/** The component shots of `AndroidS3KeyFields`, one for each row of its preview. */
class AndroidS3KeyFieldsShots : BoardShot() {
    @Test
    fun androidComponentS3KeyFieldsEmpty() = component("android.component.s3-key-fields.empty") { KeyFields("", "") }

    @Test
    fun androidComponentS3KeyFieldsTyped() = component("android.component.s3-key-fields.typed") { KeyFields(KEY_ID, SECRET) }

    @Test
    fun androidComponentS3KeyFieldsIdFocused() =
        component("android.component.s3-key-fields.id-focused") { KeyFields(KEY_ID, SECRET, idFocused = true) }

    @Test
    fun androidComponentS3KeyFieldsRefused() =
        component("android.component.s3-key-fields.refused") {
            KeyFields(KEY_ID, SECRET, refused = true)
        }
}

private const val KEY_ID = "AKIA4EXAMPLENEW"

private const val SECRET = "abcdefghijkl"

/** The fields, with the focus on the one the person types in next, unless [idFocused] puts it on the access key ID's. */
@Composable
private fun KeyFields(
    id: String,
    secret: String,
    idFocused: Boolean = false,
    refused: Boolean = false,
) = Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.group)) {
    AndroidS3KeyFields(
        accessKeyId = id,
        secretAccessKey = secret,
        onAccessKeyIdChange = {},
        onSecretAccessKeyChange = {},
        refused = refused,
        accessKeyIdModifier = if (idFocused) Modifier.focusOnShow() else Modifier,
        focusesNext = !idFocused,
    )
}
