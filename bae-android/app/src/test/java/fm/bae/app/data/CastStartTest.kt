package fm.bae.app.data

import android.app.Application
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeRendererKind

@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class CastStartTest {
    private val scope = TestScope(UnconfinedTestDispatcher())
    private val kitchen =
        BridgeCastReceiver(
            id = "kitchen",
            name = "Kitchen",
            kind = BridgeRendererKind.AIR_PLAY,
            model = null,
            address = "192.168.1.40",
            takesVolume = true,
        )
    private val den =
        BridgeCastReceiver(
            id = "den",
            name = "Den",
            kind = BridgeRendererKind.DLNA,
            model = null,
            address = "192.168.1.40",
            takesVolume = true,
        )
    private val told = mutableListOf<DisplayError>()

    private fun failingStart(pickerOpen: Boolean) =
        CastStart(
            scope = scope,
            castTo = { throw BridgeException.Diagnostic(BridgeErrorCategory.Network, "unreachable") },
            failureOf = { DisplayError("Couldn't reach it", null) },
            pickerOpen = { pickerOpen },
            tell = { told += it },
        )

    @Test
    fun aCastThatFailsWhileThePickerIsOpenShowsWhyInThePickerAndStopsConnecting() {
        val start = failingStart(pickerOpen = true)

        start.cast(kitchen)

        assertNull(start.connecting.value)
        assertEquals("Couldn't reach it", start.failure.value?.line)
        assertEquals(emptyList<DisplayError>(), told)
        assertNull(start.started.value)
    }

    @Test
    fun aCastThatFailsAfterThePickerClosedIsToldNotHeld() {
        val start = failingStart(pickerOpen = false)

        start.cast(kitchen)

        assertNull(start.connecting.value)
        assertNull(start.failure.value)
        assertEquals(listOf("Couldn't reach it"), told.map { it.line })
    }

    @Test
    fun theFailureThePickerShowedGoesWhenItCloses() {
        val start = failingStart(pickerOpen = true)
        start.cast(kitchen)

        start.clearFailure()

        assertNull(start.failure.value)
    }

    @Test
    fun aNewerCastCallsOffTheOneStillConnecting() {
        val waiting = CompletableDeferred<Unit>()
        val reached = mutableListOf<String>()
        val start =
            CastStart(
                scope = scope,
                castTo = { id ->
                    if (id == "kitchen") waiting.await()
                    reached += id
                },
                failureOf = { null },
                pickerOpen = { true },
                tell = { told += it },
            )

        val kitchenAsked = start.cast(kitchen)
        assertEquals("Kitchen", start.connecting.value?.name)
        val denAsked = start.cast(den)
        waiting.complete(Unit)

        assertEquals(listOf("den"), reached)
        assertTrue(start.didStart(denAsked))
        assertFalse(start.didStart(kitchenAsked))
        assertNull(start.connecting.value)
    }

    @Test
    fun aCastThatConnectsClosesOnlyThePlaceThatAskedForIt() {
        val waiting = CompletableDeferred<Unit>()
        val start =
            CastStart(
                scope = scope,
                castTo = { waiting.await() },
                failureOf = { null },
                pickerOpen = { true },
                tell = { told += it },
            )

        // A picker asks, is closed, and a second opens before the connect finishes: the second asked nothing.
        val firstPickerAsked = start.cast(kitchen)
        val secondPickerAsked: CastStart.Attempt? = null
        assertFalse(start.didStart(firstPickerAsked))
        waiting.complete(Unit)

        assertTrue(start.didStart(firstPickerAsked))
        assertFalse(start.didStart(secondPickerAsked))
    }
}
