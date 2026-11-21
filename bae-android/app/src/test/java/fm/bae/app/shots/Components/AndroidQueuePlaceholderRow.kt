package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import fm.bae.app.ui.playback.AndroidQueuePlaceholderRow
import fm.bae.app.ui.playback.AndroidQueueRow
import org.junit.Test

/** The component shots of `AndroidQueuePlaceholderRow`, one for each row of its preview. */
class AndroidQueuePlaceholderRowShots : BoardShot() {
    @Test
    fun androidComponentQueuePlaceholderRowNotLoaded() =
        component("android.component.queue-placeholder-row.not-loaded") { AndroidQueuePlaceholderRow() }

    @Test
    fun androidComponentQueuePlaceholderRowBesideALoadedRow() =
        component("android.component.queue-placeholder-row.beside-a-loaded-row") {
            Images {
                Column {
                    AndroidQueueRow(
                        title = "Margins",
                        artist = "Mara Quill",
                        cover = playing.cover,
                        album = "Northbound",
                        duration = "4:55",
                    )
                    AndroidQueuePlaceholderRow()
                }
            }
        }
}
