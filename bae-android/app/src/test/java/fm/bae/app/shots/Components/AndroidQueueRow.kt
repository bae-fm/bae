package fm.bae.app.shots

import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.playback.AndroidQueueRow
import org.junit.Test

/** The component shots of `AndroidQueueRow`, one for each row of its preview. */
class AndroidQueueRowShots : BoardShot() {
    @Test
    fun androidComponentQueueRowQueued() =
        component("android.component.queue-row.queued") {
            OnTheSheet {
                Covers {
                    AndroidQueueRow(
                        title = "Margins",
                        artist = "Mara Quill",
                        cover = PreviewData.imageRef("Northbound"),
                        album = "Northbound",
                        duration = "4:55",
                    )
                }
            }
        }

    @Test
    fun androidComponentQueueRowNowPlaying() =
        component("android.component.queue-row.now-playing") {
            OnTheSheet {
                Covers {
                    AndroidQueueRow(
                        title = "Low Tide Hours",
                        artist = "Mara Quill",
                        cover = PreviewData.imageRef("Low Tide Hours"),
                        current = true,
                    )
                }
            }
        }

    @Test
    fun androidComponentQueueRowDragging() =
        component("android.component.queue-row.dragging") {
            OnTheSheet {
                Covers {
                    AndroidQueueRow(
                        title = "Margins",
                        artist = "Mara Quill",
                        cover = PreviewData.imageRef("Northbound"),
                        album = "Northbound",
                        duration = "4:55",
                    )
                }
            }
        }
}
