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

    @Test
    fun theListingIsHeldFromItsFirstValueAndFollowsEachNewOne() {
        store.activate()
        assertTrue(store.loading)
        assertFalse(store.hasRows)

        reads.single().emit(BridgeFixtures.playlistListing(listOf(BridgeFixtures.playlistSummary("a"))))
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse(store.loading)
        assertEquals(1, store.totalCount)

        reads.single().emit(
            BridgeFixtures.playlistListing(
                listOf(BridgeFixtures.playlistSummary("a"), BridgeFixtures.playlistSummary("b")),
            ),
        )
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(2, store.totalCount)
        assertTrue(store.hasRows)
    }

    @Test
    fun aListingWithNoPlaylistsHoldsNoRows() {
        store.activate()
        reads.single().emit(BridgeFixtures.playlistListing(listed = emptyList()))
        shadowOf(Looper.getMainLooper()).idle()

        assertFalse(store.loading)
        assertFalse(store.hasRows)
        assertEquals(0, store.totalCount)
    }

    @Test
    fun aFailedReadSaysSoUntilRetryOpensAnother() {
        store.activate()
        reads.single().fail(BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed"))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals("Could not load your library.", store.error?.error?.line)
        assertFalse(store.loading)

        store.error!!.onRetry()
        assertNull(store.error)
        assertEquals(2, reads.size)
        reads.last().emit(BridgeFixtures.playlistListing())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1, store.totalCount)
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
        assertTrue(store.loading)
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
