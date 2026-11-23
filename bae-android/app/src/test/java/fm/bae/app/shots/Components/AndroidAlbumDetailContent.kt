package fm.bae.app.shots

import fm.bae.app.ui.albumdetail.AndroidAlbumDetailContent
import fm.bae.app.ui.albumdetail.block
import org.junit.Test

/** The component shots of `AndroidAlbumDetailContent`, one for each row of its preview. */
class AndroidAlbumDetailContentShots : BoardShot() {
    @Test
    fun androidComponentAlbumDetailContentResting() =
        component("android.component.album-detail-content.resting") {
            AndroidAlbumDetailContent {
                block { PreviewSlot("The header") }
                block { PreviewSlot("The tracks") }
            }
        }
}
