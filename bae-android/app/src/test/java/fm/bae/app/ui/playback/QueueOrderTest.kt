package fm.bae.app.ui.playback

import fm.bae.app.playback.QueueContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeTrackDisplay

class QueueOrderTest {
    private fun entry(id: String) =
        BridgeQueueEntry(
            entryId = id,
            trackId = "track-$id",
            display =
                BridgeTrackDisplay(
                    title = "Track Title",
                    artistNames = "Artist Name",
                    albumId = "album-1",
                    releaseId = "release-1",
                    albumTitle = "Album Title",
                    coverImage = null,
                ),
            durationClock = BridgeDurationClock(negative = false, hours = null, minutes = 3u, seconds = 0u),
        )

    private fun context(
        first: List<String>,
        total: Int,
        paged: Map<Int, String> = emptyMap(),
    ) = QueueContext(
        kind = BridgePlaybackSourceKind.LIBRARY,
        shuffled = true,
        upcoming = first.map(::entry),
        upcomingTotal = total,
        pagedUpcoming = paged.mapValues { entry(it.value) },
    )

    private fun ContextLaneOrder.shown(): List<String?> = (0 until size).map { itemAt(it)?.entryId }

    @Test
    fun theLaneIsSizedToTheWholeTailAndReadsEachRowFromItsPages() {
        val lane = ContextLaneOrder()
        lane.seed(context(first = listOf("a", "b"), total = 5, paged = mapOf(4 to "e")))
        assertEquals(listOf("a", "b", null, null, "e"), lane.shown())
    }

    @Test
    fun aDragShowsItsMovesAndTheValueAfterItReseedsTheLane() {
        val order = QueueOrder()
        order.apply(emptyList(), context(first = listOf("a", "b", "c", "d"), total = 6), revision = 1u)
        order.dragging()
        order.context.move(from = 0, to = 2)
        assertEquals(listOf("b", "c", "a", "d", null, null), order.context.shown())
        assertEquals(2, order.context.indexOf("a"))
        order.context.move(from = 3, to = 1)
        assertEquals(listOf("b", "d", "c", "a", null, null), order.context.shown())
        assertEquals(3, order.context.indexOf("a"))

        // The value after the drag is core's order, whether or not the reorder took.
        order.apply(emptyList(), context(first = listOf("a", "b", "c", "d"), total = 6), revision = 1u)
        assertEquals(listOf("a", "b", "c", "d", null, null), order.context.shown())
    }

    @Test
    fun aPageOfTheSameRevisionFillsOnlyItsRows() {
        val order = QueueOrder()
        order.apply(emptyList(), context(first = listOf("a", "b"), total = 4), revision = 1u)
        order.apply(emptyList(), context(first = listOf("a", "b"), total = 4, paged = mapOf(3 to "d")), revision = 1u)
        assertEquals(listOf("a", "b", null, "d"), order.context.shown())

        order.apply(emptyList(), context(first = listOf("b", "d"), total = 3), revision = 2u)
        assertEquals(listOf("b", "d", null), order.context.shown())
        assertNull(order.context.indexOf("a"))
    }
}
