package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.playback.AndroidQueuePlaceholderRow
import fm.bae.app.ui.playback.AndroidQueueRow
import org.junit.Test

/** The component shots of `AndroidQueuePlaceholderRow`, one for each row of its preview. */
class AndroidQueuePlaceholderRowShots : BaeBoardShot() {
    @Test
    fun androidComponentQueuePlaceholderRowNotLoaded() =
        component("android.component.queue-placeholder-row.not-loaded") { OnTheSheet { AndroidQueuePlaceholderRow() } }

    @Test
    fun androidComponentQueuePlaceholderRowBesideALoadedRow() =
        component("android.component.queue-placeholder-row.beside-a-loaded-row") {
            OnTheSheet {
                Covers {
                    Column {
                        AndroidQueueRow(
                            title = "Margins",
                            artist = "Mara Quill",
                            cover = PreviewData.imageRef("Northbound"),
                            album = "Northbound",
                            duration = "4:55",
                        )
                        AndroidQueuePlaceholderRow()
                    }
                }
            }
        }
}
