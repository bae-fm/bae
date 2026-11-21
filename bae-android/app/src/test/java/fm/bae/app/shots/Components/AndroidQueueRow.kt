package fm.bae.app.shots

import fm.bae.app.ui.playback.AndroidQueueRow
import org.junit.Test

/** The component shots of `AndroidQueueRow`, one for each row of its preview. */
class AndroidQueueRowShots : BoardShot() {
    @Test
    fun androidComponentQueueRowQueued() =
        component("android.component.queue-row.queued") {
            Images {
                AndroidQueueRow(
                    title = "Margins",
                    artist = "Mara Quill",
                    cover = playing.cover,
                    album = "Northbound",
                    duration = "4:55",
                )
            }
        }

    @Test
    fun androidComponentQueueRowNowPlaying() =
        component("android.component.queue-row.now-playing") {
            Images {
                AndroidQueueRow(title = "Low Tide Hours", artist = "Mara Quill", cover = playing.cover, current = true)
            }
        }
}
