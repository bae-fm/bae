package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidPageActions
import fm.bae.app.ui.components.QueueActions
import org.junit.Test

/** The component shots of `AndroidPageActions`, one for each row of its preview. */
class AndroidPageActionsShots : BoardShot() {
    @Test
    fun androidComponentPageActionsAlbum() =
        component("android.component.page-actions.album") {
            AndroidPageActions(onPlay = {}, onShuffle = {}, queue = QueueActions({}, {}))
        }

    @Test
    fun androidComponentPageActionsPlaylist() =
        component("android.component.page-actions.playlist") { AndroidPageActions(onPlay = {}, onShuffle = {}) }
}
