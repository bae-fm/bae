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
import fm.bae.app.ui.settings.AndroidRestoreCode
import fm.bae.app.ui.settings.RestoreCodeState
import org.junit.Test

private const val KEY_ID = "AKIA4EXAMPLENEW"
private const val SECRET = "abcdefghijkl"

/** The component shots of the settings fields: the S3 access key's two fields and the library's restore code. */
class SettingsFieldComponentShots : BoardShot() {
    @Test
    fun androidComponentS3KeyFieldsEmpty() = component("android.component.s3-key-fields.empty", wide = true) { KeyFields("", "") }

    @Test
    fun androidComponentS3KeyFieldsTyped() = component("android.component.s3-key-fields.typed", wide = true) { KeyFields(KEY_ID, SECRET) }

    @Test
    fun androidComponentS3KeyFieldsIdFocused() =
        component("android.component.s3-key-fields.id-focused", wide = true) {
            val focus = remember { FocusRequester() }
            KeyFields(KEY_ID, SECRET, idModifier = Modifier.focusRequester(focus))
            LaunchedEffect(Unit) { focus.requestFocus() }
        }

    @Test
    fun androidComponentS3KeyFieldsRefused() =
        component("android.component.s3-key-fields.refused", wide = true) {
            KeyFields(KEY_ID, SECRET, note = "S3 refused this key.")
        }

    @Test
    fun androidComponentRestoreCodeHidden() =
        component("android.component.restore-code.hidden", wide = true) { RestoreCode(RestoreCodeState.Masked(RESTORE_CODE), s3 = false) }

    @Test
    fun androidComponentRestoreCodeShown() =
        component("android.component.restore-code.shown", wide = true) {
            RestoreCode(RestoreCodeState.Shown(SHOWN_RESTORE_CODE), s3 = false)
        }

    @Test
    fun androidComponentRestoreCodeS3() =
        component("android.component.restore-code.s3", wide = true) { RestoreCode(RestoreCodeState.Masked(RESTORE_CODE), s3 = true) }
}

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

@Composable
private fun RestoreCode(
    state: RestoreCodeState,
    s3: Boolean,
) = AndroidRestoreCode(needsS3Key = s3, state = state, onShow = {}, onHide = {}, onPrint = {})
