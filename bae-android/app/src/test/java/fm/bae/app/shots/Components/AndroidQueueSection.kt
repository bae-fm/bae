package fm.bae.app.shots

import fm.bae.app.ui.playback.AndroidQueueSection
import org.junit.Test

/** The component shots of `AndroidQueueSection`, one for each row of its preview. */
class AndroidQueueSectionShots : BoardShot() {
    @Test
    fun androidComponentQueueSectionUpNext() =
        component("android.component.queue-section.up-next") { AndroidQueueSection("Up next", "Clear up next", {}) }

    @Test
    fun androidComponentQueueSectionPlayingFrom() =
        component("android.component.queue-section.playing-from") {
            AndroidQueueSection("Playing from", "Clear playing from", {}, shuffled = false)
        }

    @Test
    fun androidComponentQueueSectionShuffleOn() =
        component("android.component.queue-section.shuffle-on") {
            AndroidQueueSection("Playing from", "Clear playing from", {}, shuffled = true)
        }
}
