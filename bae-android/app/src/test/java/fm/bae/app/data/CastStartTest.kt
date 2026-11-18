package fm.bae.app.data

import android.app.Application
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAirPlayCode
import uniffi.bae_bridge.BridgeAirPlayCodeAsked
import uniffi.bae_bridge.BridgeAirPlayCodeRefused
import uniffi.bae_bridge.BridgeAirPlayCodeRequest
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
            castTo = { _, _ -> throw BridgeException.Diagnostic(BridgeErrorCategory.Network, "unreachable") },
            failureOf = { CastStart.Failure(DisplayError("Couldn't reach it", null), refusedBy = null) },
            pickerOpen = { pickerOpen },
            tell = { told += it },
        )

    @Test
    fun aCastThatFailsWhileThePickerIsOpenShowsWhyInThePickerAndStopsConnecting() {
        val start = failingStart(pickerOpen = true)

        start.cast(kitchen)

        assertNull(start.connecting.value)
        assertEquals(
            "Couldn't reach it",
            start.failure.value
                ?.error
                ?.line,
        )
        assertNull(start.failure.value?.refusedBy)
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
                castTo = { id, _ ->
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
                castTo = { _, _ -> waiting.await() },
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

    private val pinRequest =
        BridgeAirPlayCodeRequest(
            code = BridgeAirPlayCode.Pin(digits = 4u),
            why = BridgeAirPlayCodeAsked.FIRST_PAIRING,
            lastTry = null,
        )

    @Test
    fun aReceiversAskOpensTheDialogAndPairHandsBackTheCode() {
        val entered = mutableListOf<String>()
        val wrong = pinRequest.copy(lastTry = BridgeAirPlayCodeRefused.WRONG_CODE)
        val finish = CompletableDeferred<Unit>()
        val start =
            CastStart(
                scope = scope,
                castTo = { _, prompt ->
                    entered += prompt.code(pinRequest)
                    entered += prompt.code(wrong)
                    finish.await()
                },
                failureOf = { null },
                pickerOpen = { true },
                tell = { told += it },
            )

        start.cast(kitchen)
        assertEquals(pinRequest, start.codeSheet.value?.request)
        assertFalse(start.codeSheet.value!!.pairing)

        start.pair("0482")
        assertEquals(wrong, start.codeSheet.value?.request)
        assertFalse(start.codeSheet.value!!.pairing)
        start.pair("1234")
        assertTrue(start.codeSheet.value!!.pairing)
        assertEquals(listOf("0482", "1234"), entered)

        finish.complete(Unit)
        assertNull(start.codeSheet.value)
        assertNull(start.connecting.value)
    }

    @Test
    fun cancelClosesTheDialogAndCallsTheCastOff() {
        val start =
            CastStart(
                scope = scope,
                castTo = { _, prompt ->
                    prompt.code(pinRequest)
                    awaitCancellation()
                },
                failureOf = { null },
                pickerOpen = { true },
                tell = { told += it },
            )

        val asked = start.cast(kitchen)
        start.cancelCode()

        assertNull(start.codeSheet.value)
        assertNull(start.connecting.value)
        assertFalse(start.didStart(asked))
    }

    @Test
    fun aReceiversRefusalKeepsTheReceiverForThePickersTitle() {
        val start =
            CastStart(
                scope = scope,
                castTo = { _, _ ->
                    throw BridgeException.Diagnostic(BridgeErrorCategory.AirPlayLockedOut("Kitchen"), "locked")
                },
                failureOf = { CastStart.Failure(DisplayError("Kitchen refuses more PIN attempts.", null), refusedBy = "Kitchen") },
                pickerOpen = { true },
                tell = { told += it },
            )

        start.cast(kitchen)

        assertEquals("Kitchen", start.failure.value?.refusedBy)
    }
}
