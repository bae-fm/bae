package fm.bae.app.data

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.NoErrorDisplays
import fm.bae.screensubscription.PulledSubscription
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaylistListing

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class PlaylistListingStoreTest {
    private val reads = mutableListOf<RecordingRead>()
    private val store =
        PlaylistListingStore(
            open = { RecordingRead().also(reads::add) },
            appContext = RuntimeEnvironment.getApplication(),
            errors = NoErrorDisplays,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
        )

    /** The failure the view draws in its rows' place, while it draws one. */
    private val PlaylistListingStore.failure: PageError? get() = (state as? ListPageState.Failed)?.error

    @Test
    fun theListingIsHeldFromItsFirstValueAndFollowsEachNewOne() {
        store.activate()
        assertEquals(ListPageState.Waiting, store.state)

        reads.single().emit(BridgeFixtures.playlistListing(listOf(BridgeFixtures.playlistSummary("a"))))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(ListPageState.Counted(1), store.state)

        reads.single().emit(
            BridgeFixtures.playlistListing(
                listOf(BridgeFixtures.playlistSummary("a"), BridgeFixtures.playlistSummary("b")),
            ),
        )
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(ListPageState.Counted(2), store.state)
    }

    @Test
    fun aListingWithNoPlaylistsHoldsNoRows() {
        store.activate()
        reads.single().emit(BridgeFixtures.playlistListing(listed = emptyList()))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(ListPageState.Counted(0), store.state)
    }

    @Test
    fun aFailedReadSaysSoUntilRetryOpensAnother() {
        store.activate()
        reads.single().fail(BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed"))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals("Could not load your library.", store.failure?.error?.line)
        assertFalse("a failed read leaves its read open", reads.single().cancelled)

        store.failure!!.onRetry()
        assertEquals(ListPageState.Waiting, store.state)
        assertEquals(2, reads.size)
        reads.last().emit(BridgeFixtures.playlistListing())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(ListPageState.Counted(1), store.state)
    }

    /** A failure replaces the listing the read held, and the read's next value replaces the failure. */
    @Test
    fun aFailureReplacesTheListingUntilTheReadAnswersAgain() {
        store.activate()
        reads.single().emit(BridgeFixtures.playlistListing(listOf(BridgeFixtures.playlistSummary("a"))))
        shadowOf(Looper.getMainLooper()).idle()

        reads.single().fail(BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed"))
        shadowOf(Looper.getMainLooper()).idle()
        assertNull(store.listing)
        assertTrue(store.state is ListPageState.Failed)

        reads.single().emit(BridgeFixtures.playlistListing(listOf(BridgeFixtures.playlistSummary("a"))))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(ListPageState.Counted(1), store.state)
    }

    @Test
    fun leavingTheViewEndsTheReadAndForgetsTheListing() {
        store.activate()
        reads.single().emit(BridgeFixtures.playlistListing())
        shadowOf(Looper.getMainLooper()).idle()

        store.deactivate()
        shadowOf(Looper.getMainLooper()).idle()

        assertTrue(reads.single().cancelled)
        assertNull(store.listing)
        assertEquals(ListPageState.Waiting, store.state)
    }

    private class RecordingRead : PulledSubscription<BridgePlaylistListing> {
        private val values = Channel<Result<BridgePlaylistListing>>(Channel.UNLIMITED)
        var cancelled = false
            private set

        override suspend fun next(): BridgePlaylistListing = values.receive().getOrThrow()

        override suspend fun cancel() {
            cancelled = true
        }

        fun emit(listing: BridgePlaylistListing) {
            values.trySend(Result.success(listing))
        }

        fun fail(error: BridgeException) {
            values.trySend(Result.failure(error))
        }
    }
}
