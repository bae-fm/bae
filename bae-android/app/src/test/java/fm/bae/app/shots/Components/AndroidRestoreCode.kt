package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.settings.AndroidRestoreCode
import fm.bae.app.ui.settings.RestoreCodeState
import org.junit.Test

/** The component shots of `AndroidRestoreCode`, one for each row of its preview. */
class AndroidRestoreCodeShots : BoardShot() {
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
private fun RestoreCode(
    state: RestoreCodeState,
    s3: Boolean,
) = AndroidRestoreCode(needsS3Key = s3, state = state, onShow = {}, onHide = {}, onPrint = {})
