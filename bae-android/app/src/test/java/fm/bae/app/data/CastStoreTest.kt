package fm.bae.app.data

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.DisplayError
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeRemotePlayback
import uniffi.bae_bridge.BridgeRendererKind

/**
 * The cast button and the picker's active row read the casting device off the
 * status core last announced, so the store must surface the latest one — a
 * receiver-side end arrives the same way a user stop does, with why.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class CastStoreTest {
    private val device =
        BridgeCastReceiver(id = "cast-2", name = "Speaker Name", kind = BridgeRendererKind.CAST, model = null, address = "192.168.1.40")

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
    fun receiversSurfaceTheLatestList() {
        val store = CastStore()
        assertTrue(store.receivers.value.isEmpty)

        store.setReceivers(
            BridgeCastReceiverList(
                casting = device,
                found =
                    listOf(
                        BridgeFixtures.castReceiver(id = "cast-1", name = "Kitchen"),
                        BridgeFixtures.castReceiver(id = "cast-3", name = "Study"),
                    ),
            ),
        )
        assertEquals(device, store.receivers.value.casting)
        assertEquals(
            listOf("Kitchen", "Study"),
            store.receivers.value.found
                .map { it.name },
        )

        store.setReceivers(BridgeCastReceiverList(casting = null, found = emptyList()))
        assertTrue(store.receivers.value.isEmpty)
    }
}
