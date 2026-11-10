package fm.bae.app.ui.playback

import android.app.Application
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
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
    private val kitchen = BridgeCastReceiver(id = "kitchen", name = "Kitchen", kind = BridgeRendererKind.AIR_PLAY, casting = false)
    private val den = BridgeCastReceiver(id = "den", name = "Den", kind = BridgeRendererKind.DLNA, casting = false)

    @Test
    fun aCastThatFailsShowsWhyAndStopsConnecting() {
        val start =
            CastStart(
                scope = scope,
                castTo = { throw BridgeException.Diagnostic(BridgeErrorCategory.Network, "unreachable") },
                failureOf = { DisplayError("Couldn't reach it", null) },
            )
        var started = false

        start.cast(kitchen) { started = true }

        assertNull(start.connecting)
        assertEquals("Couldn't reach it", start.failure?.line)
        assertEquals(false, started)
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
            )
        val started = mutableListOf<String>()

        start.cast(kitchen) { started += "kitchen" }
        assertEquals("Kitchen", start.connecting?.name)
        start.cast(den) { started += "den" }
        waiting.complete(Unit)

        assertEquals(listOf("den"), reached)
        assertEquals(listOf("den"), started)
        assertNull(start.connecting)
    }
}
