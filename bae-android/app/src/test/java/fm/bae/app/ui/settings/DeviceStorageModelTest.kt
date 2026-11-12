package fm.bae.app.ui.settings

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.data.LiveValue
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.FakeDeviceStorageSubscription
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.DeviceStorageSubscription

/**
 * What the phone keeps is none before core's first measurement; a failed read replaces the value and the next
 * measurement replaces the failure; a cancelled read shows nothing and leaves the value as it was.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class DeviceStorageModelTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** Compose's snapshot apply is posted to the main looper, which Robolectric drops when the test ends. */
    @After
    fun runWhatTheTestLeftOnTheMainLooper() {
        shadowOf(Looper.getMainLooper()).idle()
    }

    @Test
    fun nothingIsKnownBeforeTheFirstMeasurement() {
        val handle = StorageHandle()
        val model = DeviceStorageModel(handle, TestErrorDisplays, scope)

        model.watch()
        shadowOf(Looper.getMainLooper()).idle()

        assertNull(model.read)
    }

    @Test
    fun eachMeasurementReplacesTheLast() {
        val handle = StorageHandle()
        val model = DeviceStorageModel(handle, TestErrorDisplays, scope)
        model.watch()

        handle.subscriptions.last().deliver(BridgeFixtures.deviceStorage())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(LiveValue.Current(BridgeFixtures.deviceStorage()), model.read)

        handle.subscriptions.last().deliver(BridgeFixtures.deviceStorage(beforeReset = 540_000_000uL))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(LiveValue.Current(BridgeFixtures.deviceStorage(beforeReset = 540_000_000uL)), model.read)
    }

    @Test
    fun aFailedReadReplacesTheValueAndTheNextMeasurementReplacesTheFailure() {
        val handle = StorageHandle()
        val model = DeviceStorageModel(handle, TestErrorDisplays, scope)
        model.watch()
        val pulls = handle.subscriptions.last()
        pulls.deliver(BridgeFixtures.deviceStorage())
        shadowOf(Looper.getMainLooper()).idle()

        pulls.fail(BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline"))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(LiveValue.Failed(DisplayError("couldn't measure")), model.read)

        pulls.deliver(BridgeFixtures.deviceStorage())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(LiveValue.Current(BridgeFixtures.deviceStorage()), model.read)
    }

    @Test
    fun aCancelledReadLeavesTheValueAsItWas() {
        val handle = StorageHandle()
        val model = DeviceStorageModel(handle, TestErrorDisplays, scope)
        model.watch()
        val pulls = handle.subscriptions.last()
        pulls.deliver(BridgeFixtures.deviceStorage())
        shadowOf(Looper.getMainLooper()).idle()

        pulls.fail(BridgeException.Cancelled())
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(LiveValue.Current(BridgeFixtures.deviceStorage()), model.read)
    }

    @Test
    fun stoppingEndsTheSubscription() {
        val handle = StorageHandle()
        val model = DeviceStorageModel(handle, TestErrorDisplays, scope)
        model.watch()
        val pulls = handle.subscriptions.last()

        model.stop()
        shadowOf(Looper.getMainLooper()).idle()
        pulls.deliver(BridgeFixtures.deviceStorage())
        shadowOf(Looper.getMainLooper()).idle()

        assertNull(model.read)
    }

    /** Core's answer for what each failure shows: a cancellation shows nothing. */
    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) = if (error is BridgeException.Cancelled) null else DisplayError("couldn't measure")

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

        override fun display(outcome: BridgePlayOutcome): DisplayError? = null
    }

    private class StorageHandle : FakeAppHandle() {
        val subscriptions = mutableListOf<FakeDeviceStorageSubscription>()

        override fun subscribeDeviceStorage(): DeviceStorageSubscription = FakeDeviceStorageSubscription().also(subscriptions::add)
    }
}
