package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.DisplayError
import fm.bae.app.ui.settings.AndroidSettingsRestoreCode
import fm.bae.app.ui.settings.RestoreCodeSectionContent
import fm.bae.app.ui.settings.RestoreCodeState
import fm.bae.app.ui.settings.listeningHistoryParts
import fm.bae.app.ui.settings.renameProfileParts
import org.junit.Test
import uniffi.bae_bridge.BridgeHistorySharing

/** The shots of the ready listening-history, your-name and restore-code boards: what a person sets about themselves. */
class SettingsYouShots : BoardShot() {
    /** The choices of who sees what Ana plays. */
    @Test
    fun androidSettingsListeningHistoryChoosing() =
        shot("android.settings.listening-history.choosing") {
            InPlaceDialogScene(behind = { PeoplePageScene(listOf(ana(), ben())) }) {
                listeningHistoryParts(current = BridgeHistorySharing.PRIVATE, onChoose = {}, onDismiss = {})
            }
        }

    /** The page after Ana shares her history with everyone. */
    @Test
    fun androidSettingsListeningHistoryChanged() =
        shot("android.settings.listening-history.changed") {
            ScreenScene { PeoplePageScene(listOf(ana(history = BridgeHistorySharing.HOUSEHOLD), ben())) }
        }

    /** The name dialog with nothing typed. */
    @Test
    fun androidSettingsYourNameBlank() = shot("android.settings.your-name.blank") { YourName(draft = "", error = null) }

    /** The name dialog with a new name typed. */
    @Test
    fun androidSettingsYourNameEditing() = shot("android.settings.your-name.editing") { YourName(draft = "Annie", error = null) }

    /** The name dialog after saving failed. */
    @Test
    fun androidSettingsYourNameSaveFailed() =
        shot("android.settings.your-name.save-failed") {
            YourName(draft = "Annie", error = DisplayError("Couldn’t save the name. Try again."))
        }

    /** The page after the name was saved. */
    @Test
    fun androidSettingsYourNameSaved() =
        shot("android.settings.your-name.saved") {
            ScreenScene { PeoplePageScene(listOf(ana(name = "Annie"), ben())) }
        }

    /** The restore code, hidden but for its first groups. */
    @Test
    fun androidSettingsRestoreCodeHidden() = shot("android.settings.restore-code.hidden") { RestoreCode(RestoreCodeState.Masked(CODE)) }

    /** The restore code, shown. */
    @Test
    fun androidSettingsRestoreCodeShown() = shot("android.settings.restore-code.shown") { RestoreCode(RestoreCodeState.Shown(CODE)) }
}

@Composable
private fun YourName(
    draft: String,
    error: DisplayError?,
) = InPlaceDialogScene(behind = { PeoplePageScene(listOf(ana(), ben())) }) {
    renameProfileParts(draft = draft, error = error, saving = false, onDraftChange = {}, onSave = {}, onDismiss = {})
}

@Composable
private fun RestoreCode(state: RestoreCodeState) =
    ScreenScene {
        AndroidSettingsRestoreCode(
            onBack = {},
            section = { RestoreCodeSectionContent(needsS3Key = false, state = state, onShow = {}, onHide = {}, onPrint = {}) },
        )
    }

private const val CODE =
    "CVR1 UJZD EGXD NCF3 2EPF 3DHO DZDO CIS2 JHTL GMXG EDN7 3U55 XTPL PFT7 V4SE H2KV J72C EUVW 75EF R6ED T4SY WB5W KH7D NSIP " +
        "ZZ7F K4ZR I3R2 WYOJ FLJO OA7L QSAJ 2XUI D5ZZ ZZG6 ZDME N4KH VDGA JGXB ENYJ QWX6 HH75 66TF JGVQ 6KBN XJBT FQXK WOVO MPZO " +
        "M7WB BR6Q MW4W XFOG O6MV N6A6 WFHY M6L3 VFZ5 ZFKK IBJ5 J6WJ IBAG I3MN B"
