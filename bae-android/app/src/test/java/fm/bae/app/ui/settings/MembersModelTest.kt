package fm.bae.app.ui.settings

import android.os.Looper
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.PreviewData
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome

/**
 * A failed read of the library's people ends the spinner with the failure, and
 * Retry reads again; a cancelled read shows nothing and leaves the list as it
 * was.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class MembersModelTest {
    @Test
    fun aFailedFirstReadShowsTheFailureAndRetryReadsAgain() {
        val handle = PeopleHandle(listOf(Result.failure(offline()), Result.success(listOf(PreviewData.person()))))
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        model.reload()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PeopleRead.Failed(DisplayError("couldn't reach storage")), model.people)

        model.retry()
        shadowOf(Looper.getMainLooper()).idle()
        val loaded = model.people as PeopleRead.Loaded
        assertEquals(listOf(PreviewData.person().membership.id), loaded.people.map { it.membership.id })
        assertEquals(2, handle.reads)
    }

    @Test
    fun aCancelledReadLeavesTheListAsItWas() {
        val handle =
            PeopleHandle(
                listOf(
                    Result.failure(BridgeException.Cancelled()),
                    Result.success(listOf(PreviewData.person())),
                    Result.failure(BridgeException.Cancelled()),
                ),
            )
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        model.reload()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PeopleRead.Loading, model.people)

        model.reload()
        shadowOf(Looper.getMainLooper()).idle()
        val loaded = model.people as PeopleRead.Loaded

        model.reload()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(loaded, model.people)
        assertEquals(3, handle.reads)
    }

    /** Core's answer for what each failure shows: a cancellation shows nothing. */
    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) =
            if (error is BridgeException.Cancelled) null else DisplayError("couldn't reach storage")

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null
    }

    private fun offline() = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

    private class PeopleHandle(
        private val answers: List<Result<List<BridgePerson>>>,
    ) : FakeAppHandle() {
        var reads = 0
            private set

        override suspend fun people(): List<BridgePerson> = answers[reads++].getOrThrow()
    }
}
