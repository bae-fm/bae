package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.DisplayError
import fm.bae.app.ui.settings.listeningHistoryParts
import fm.bae.app.ui.settings.renameProfileParts
import org.junit.Test
import uniffi.bae_bridge.BridgeHistorySharing

/** The shots of the ready listening-history and your-name boards: what a person sets about themselves. */
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
            YourName(draft = "Annie", error = DisplayError("The library could not be written.", "profile write: no space left on device"))
        }

    /** The page after the name was saved. */
    @Test
    fun androidSettingsYourNameSaved() =
        shot("android.settings.your-name.saved") {
            ScreenScene { PeoplePageScene(listOf(ana(name = "Annie"), ben())) }
        }
}

@Composable
private fun YourName(
    draft: String,
    error: DisplayError?,
) = InPlaceDialogScene(behind = { PeoplePageScene(listOf(ana(), ben())) }) {
    renameProfileParts(draft = draft, error = error, saving = false, onDraftChange = {}, onSave = {}, onDismiss = {})
}
