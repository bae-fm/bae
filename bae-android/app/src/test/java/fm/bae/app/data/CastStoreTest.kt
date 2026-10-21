package fm.bae.app.data

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import fm.bae.app.BridgeFixtures
import fm.bae.app.DisplayError
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeRemoteDevice
import uniffi.bae_bridge.BridgeRemotePlayback

/**
 * The cast button and the picker's active row read the casting device off the
 * status core last announced, so the store must surface the latest one — a
 * receiver-side end arrives the same way a user stop does, with why.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class CastStoreTest {
    private val device = BridgeRemoteDevice(id = "cast-2", name = "Speaker Name")

    @Test
    fun statusCarriesTheCastingDevice() {
        val store = CastStore()
        assertNull(castingDevice(store.status.value))

        store.applyStatus(BridgeRemotePlayback.On(device))
        assertEquals(device, castingDevice(store.status.value))
        assertEquals("Speaker Name", castingDeviceName(store.status.value))

        store.applyStatus(BridgeRemotePlayback.Local)
        assertNull(castingDevice(store.status.value))
    }

    /** A device whose link was lost is no longer casting, and its end names it with core's reason behind it. */
    @Test
    fun aLostDeviceIsNotCastingAndSaysWhy() {
        val context = ApplicationProvider.getApplicationContext<Application>()
        val ended = BridgeRemotePlayback.Ended(device, "receiver closed the session")

        assertNull(castingDevice(ended))
        assertEquals(
            DisplayError("Playback on Speaker Name stopped.", "receiver closed the session"),
            context.castEndedFailure(ended),
        )
        assertNull(context.castEndedFailure(BridgeRemotePlayback.On(device)))
    }

    /** Every playback value repeats the status, so a lost link is reported once, when it is new. */
    @Test
    fun aLostLinkIsReportedOnceWhenItIsNew() {
        val store = CastStore()
        val ended = BridgeRemotePlayback.Ended(device, "receiver closed the session")
        store.applyStatus(BridgeRemotePlayback.On(device))

        assertEquals(ended, store.applyStatus(ended))
        assertNull(store.applyStatus(BridgeRemotePlayback.Ended(device, "receiver closed the session")))
        assertNull(store.applyStatus(BridgeRemotePlayback.Local))
    }

    @Test
    fun devicesSurfaceTheLatestList() {
        val store = CastStore()
        assertTrue(store.devices.value.isEmpty())

        store.setDevices(
            listOf(
                BridgeFixtures.castDevice(id = "cast-1", name = "Kitchen"),
                BridgeFixtures.castDevice(id = "cast-2", name = "Study"),
            ),
        )
        assertEquals(listOf("Kitchen", "Study"), store.devices.value.map { it.name })

        store.setDevices(emptyList())
        assertTrue(store.devices.value.isEmpty())
    }
}
