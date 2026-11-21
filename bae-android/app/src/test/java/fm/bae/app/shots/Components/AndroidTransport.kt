package fm.bae.app.shots

import fm.bae.app.ui.playback.AndroidTransport
import org.junit.Test

/** The component shots of `AndroidTransport`, one for each row of its preview. */
class AndroidTransportShots : BoardShot() {
    @Test
    fun androidComponentTransportPaused() = component("android.component.transport.paused") { AndroidTransport(false, false, {}, {}, {}) }

    @Test
    fun androidComponentTransportPlaying() = component("android.component.transport.playing") { AndroidTransport(true, false, {}, {}, {}) }

    @Test
    fun androidComponentTransportWaiting() = component("android.component.transport.waiting") { AndroidTransport(true, true, {}, {}, {}) }

    @Test
    fun androidComponentTransportLarge() =
        component("android.component.transport.large", wide = true) { AndroidTransport(true, false, {}, {}, {}, full = true) }
}
