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
        total: Int,
        read: Map<Int, String> = emptyMap(),
    ) = QueueContext(
        kind = BridgePlaybackSourceKind.LIBRARY,
        shuffled = true,
        upcomingTotal = total,
        upcoming = read.mapValues { entry(it.value) },
    )

    private fun rows(vararg ids: String): Map<Int, String> = ids.withIndex().associate { it.index to it.value }

    private fun ContextLaneOrder.shown(): List<String?> = (0 until size).map { itemAt(it)?.entryId }

    @Test
    fun theLaneIsSizedToTheWholeTailAndShowsEachReadRow() {
        val lane = ContextLaneOrder()
        lane.seed(context(total = 5, read = rows("a", "b") + mapOf(4 to "e")))
        assertEquals(listOf("a", "b", null, null, "e"), lane.shown())
    }

    @Test
    fun aDragShowsItsMovesAndTheValueAfterItReseedsTheLane() {
        val order = QueueOrder()
        order.apply(emptyList(), context(total = 6, read = rows("a", "b", "c", "d")), manualChanges = 1u, contextChanges = 1u)
        order.dragging()
        order.context.move(from = 0, to = 2)
        assertEquals(listOf("b", "c", "a", "d", null, null), order.context.shown())
        assertEquals(2, order.context.indexOf("a"))
        order.context.move(from = 3, to = 1)
        assertEquals(listOf("b", "d", "c", "a", null, null), order.context.shown())
        assertEquals(3, order.context.indexOf("a"))

        // The value after the drag is core's order, whether or not the reorder took.
        order.apply(emptyList(), context(total = 6, read = rows("a", "b", "c", "d")), manualChanges = 1u, contextChanges = 1u)
        assertEquals(listOf("a", "b", "c", "d", null, null), order.context.shown())
    }

    @Test
    fun moreOfAnUnchangedTailFillsOnlyItsRowsAndAChangeReseedsIt() {
        val order = QueueOrder()
        order.apply(emptyList(), context(total = 4, read = rows("a", "b")), manualChanges = 1u, contextChanges = 1u)
        order.context.move(from = 0, to = 1)
        order.apply(
            emptyList(),
            context(total = 4, read = rows("a", "b") + mapOf(3 to "d")),
            manualChanges = 1u,
            contextChanges = 1u,
        )
        assertEquals("a scroll keeps the moves over the lane", listOf("b", "a", null, "d"), order.context.shown())

        order.apply(emptyList(), context(total = 3, read = rows("b", "d")), manualChanges = 1u, contextChanges = 2u)
        assertEquals(listOf("b", "d", null), order.context.shown())
        assertNull(order.context.indexOf("a"))
    }

    @Test
    fun aChangeToOneLaneReseedsOnlyThatLane() {
        val order = QueueOrder()
        order.apply(listOf(entry("m")), context(total = 2, read = rows("a", "b")), manualChanges = 1u, contextChanges = 1u)
        order.context.move(from = 0, to = 1)
        order.apply(listOf(entry("n")), context(total = 2, read = rows("a", "b")), manualChanges = 2u, contextChanges = 1u)
        assertEquals(listOf("n"), order.manual.map { it.entryId })
        assertEquals("the context lane keeps its moves", listOf("b", "a"), order.context.shown())
    }
}
