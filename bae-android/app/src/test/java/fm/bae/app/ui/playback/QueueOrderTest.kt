package fm.bae.app.ui.playback

import fm.bae.app.creditLineNamed
import fm.bae.app.playback.QueueContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeQueueLane
import uniffi.bae_bridge.BridgeTrackDisplay

class QueueOrderTest {
    private fun entry(id: String) =
        BridgeQueueEntry(
            entryId = id,
            trackId = "track-$id",
            display =
                BridgeTrackDisplay(
                    title = "Track Title",
                    artistLine = creditLineNamed("Artist Name"),
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
        upcomingTotal = total,
        upcoming = read.mapValues { entry(it.value) },
    )

    private fun rows(vararg ids: String): Map<Int, String> = ids.withIndex().associate { it.index to it.value }

    private fun ContextLaneOrder.shown(): List<String?> = (0 until size).map { itemAt(it)?.entryId }

    /** Place every row the sheet draws now, as the lazy list places the rows on screen. */
    private fun QueueOrder.placeRows() {
        manual.forEachIndexed { index, row -> placed(row.entryId, RowPlace(BridgeQueueLane.MANUAL, index)) }
        context.shown().forEachIndexed { index, id ->
            if (id != null) placed(id, RowPlace(BridgeQueueLane.CONTEXT, index))
        }
    }

    /** Start dragging [entryId] of [lane], drawn at [index]. */
    private fun QueueOrder.start(
        lane: BridgeQueueLane,
        entryId: String,
        index: Int,
    ) {
        placeRows()
        started(entryId, RowPlace(lane, index))
    }

    /** A step of the drag onto the row [onto], with the rows placed where the sheet draws them now. */
    private fun QueueOrder.stepOnto(onto: String): LaneDrop? {
        placeRows()
        return step(onto)
    }

    private fun drop(
        lane: BridgeQueueLane,
        entryId: String,
        position: Int,
    ) = LaneDrop(entryId, RowPlace(lane, position))

    @Test
    fun theLaneIsSizedToTheWholeTailAndShowsEachReadRow() {
        val lane = ContextLaneOrder()
        lane.seed(context(total = 5, read = rows("a", "b") + mapOf(4 to "e")))
        assertEquals(listOf("a", "b", null, null, "e"), lane.shown())
    }

    @Test
    fun aDragShowsItsMovesUntilItsReorderReturns() {
        val order = QueueOrder()
        order.apply(emptyList(), context(total = 6, read = rows("a", "b", "c", "d")))
        order.dragging()
        order.start(BridgeQueueLane.CONTEXT, "a", 0)
        order.stepOnto("c")
        assertEquals(listOf("b", "c", "a", "d", null, null), order.context.shown())

        // The drag ends before the reorder returns: the moves stay on screen, whatever core reads.
        order.apply(emptyList(), context(total = 6, read = rows("a", "b", "c", "d")))
        assertEquals(listOf("b", "c", "a", "d", null, null), order.context.shown())
        order.released()
        order.apply(emptyList(), context(total = 6, read = rows("a", "c", "b", "d")))
        assertEquals(listOf("b", "c", "a", "d", null, null), order.context.shown())

        order.reorderReturned(BridgeQueueLane.CONTEXT)
        assertEquals(
            "the lane shows core's read once the reorder returns, without waiting for it to match",
            listOf("a", "c", "b", "d", null, null),
            order.context.shown(),
        )
        order.apply(emptyList(), context(total = 6, read = rows("b", "c", "a", "d")))
        assertEquals("nothing holds the lane now", listOf("b", "c", "a", "d", null, null), order.context.shown())
    }

    @Test
    fun aReorderThatFailsShowsTheLanesReadWhenItReturns() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b"), entry("c")), null)
        order.dragging()
        order.start(BridgeQueueLane.MANUAL, "a", 0)
        order.stepOnto("c")
        order.released()
        order.apply(listOf(entry("a"), entry("b"), entry("c")), null)
        assertEquals("the hold waits for the reorder", listOf("b", "c", "a"), order.manual.map { it.entryId })

        order.reorderReturned(BridgeQueueLane.MANUAL)
        assertEquals("a failed move never stays on screen", listOf("a", "b", "c"), order.manual.map { it.entryId })
    }

    @Test
    fun anEarlierDragsReturnLeavesALaterDragsHold() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b"), entry("c")), null)
        order.dragging()
        order.start(BridgeQueueLane.MANUAL, "a", 0)
        order.stepOnto("b")
        order.released()
        order.dragging()
        order.start(BridgeQueueLane.MANUAL, "c", 2)
        order.stepOnto("a")
        order.released()
        order.apply(listOf(entry("a"), entry("b"), entry("c")), null)

        order.reorderReturned(BridgeQueueLane.MANUAL)
        assertEquals("the second reorder has not returned yet", listOf("b", "c", "a"), order.manual.map { it.entryId })
        order.reorderReturned(BridgeQueueLane.MANUAL)
        assertEquals(listOf("a", "b", "c"), order.manual.map { it.entryId })
    }

    @Test
    fun aReturnDuringAnotherDragWaitsForTheDragToEnd() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b")), context(total = 2, read = rows("x", "y")))
        order.dragging()
        order.start(BridgeQueueLane.MANUAL, "a", 0)
        order.stepOnto("b")
        order.released()
        order.dragging()
        order.start(BridgeQueueLane.CONTEXT, "x", 0)
        order.stepOnto("y")
        order.reorderReturned(BridgeQueueLane.MANUAL)
        assertEquals("a drag in progress keeps every lane as it is", listOf("b", "a"), order.manual.map { it.entryId })
        assertEquals(listOf("y", "x"), order.context.shown())

        order.apply(listOf(entry("b"), entry("a")), context(total = 2, read = rows("x", "y")))
        assertEquals(listOf("b", "a"), order.manual.map { it.entryId })
        assertEquals("the context lane's reorder has not returned yet", listOf("y", "x"), order.context.shown())
    }

    @Test
    fun eachReadOfTheTailShowsTheRowsItReadAndNoMore() {
        val order = QueueOrder()
        order.apply(emptyList(), context(total = 4, read = rows("a", "b")))
        order.apply(emptyList(), context(total = 4, read = rows("a", "b") + mapOf(3 to "d")))
        assertEquals("a scroll fills the rows it read", listOf("a", "b", null, "d"), order.context.shown())

        order.apply(emptyList(), context(total = 3, read = rows("b", "d")))
        assertEquals(listOf("b", "d", null), order.context.shown())
    }

    @Test
    fun aHoldOverOneLaneLeavesTheOtherFollowingCore() {
        val order = QueueOrder()
        order.apply(listOf(entry("m")), context(total = 2, read = rows("a", "b")))
        order.dragging()
        order.start(BridgeQueueLane.CONTEXT, "a", 0)
        order.stepOnto("b")
        order.apply(listOf(entry("n")), context(total = 2, read = rows("a", "b")))
        assertEquals(listOf("n"), order.manual.map { it.entryId })
        assertEquals("the context lane keeps its moves", listOf("b", "a"), order.context.shown())
    }

    @Test
    fun aDropTellsCoreThePlaceTheRowNowSitsAtInItsLane() {
        val order = QueueOrder()
        order.apply(
            listOf(entry("m"), entry("n")),
            context(total = 4, read = rows("a", "b", "c")),
        )
        order.start(BridgeQueueLane.MANUAL, "m", 0)
        assertEquals(drop(BridgeQueueLane.MANUAL, "m", 1), order.stepOnto("n"))
        assertEquals(listOf("n", "m"), order.manual.map { it.entryId })
        assertNull("a step onto a row of the other lane moves nothing", order.stepOnto("a"))
        order.released()

        order.start(BridgeQueueLane.CONTEXT, "a", 0)
        assertEquals(drop(BridgeQueueLane.CONTEXT, "a", 1), order.stepOnto("b"))
        assertEquals(listOf("b", "a", "c", null), order.context.shown())
        assertEquals(
            "a drop just before a row not read yet moves, and core finds that row",
            drop(BridgeQueueLane.CONTEXT, "a", 2),
            order.stepOnto("c"),
        )
        assertEquals(listOf("b", "c", "a", null), order.context.shown())
        assertNull("a step onto a section header moves nothing", order.stepOnto("ctxhdr"))
    }

    @Test
    fun aDropAfterTheLastRowSitsAtTheLanesLastPlace() {
        val order = QueueOrder()
        order.apply(emptyList(), context(total = 3, read = rows("a", "b", "c")))
        order.start(BridgeQueueLane.CONTEXT, "a", 0)
        assertEquals(drop(BridgeQueueLane.CONTEXT, "a", 2), order.stepOnto("c"))
        assertEquals(listOf("b", "c", "a"), order.context.shown())
    }

    @Test
    fun aDragSendsCoreOneReorderAtItsReleaseHoldingTheLaneThroughIt() {
        val order = QueueOrder()
        order.apply(listOf(entry("m"), entry("n"), entry("o"), entry("p")), context(total = 0))
        order.dragging()
        // The held row passes "n" then "o": the rows move on screen, and nothing is sent.
        order.start(BridgeQueueLane.MANUAL, "m", 0)
        order.stepOnto("n")
        order.stepOnto("o")
        assertEquals(listOf("n", "o", "m", "p"), order.manual.map { it.entryId })
        // The drag's end re-seeds the lanes from core's read, which the lane's hold keeps from undoing the moves.
        order.apply(listOf(entry("m"), entry("n"), entry("o"), entry("p")), context(total = 0))
        assertEquals("the moves stay until the reorder returns", listOf("n", "o", "m", "p"), order.manual.map { it.entryId })

        assertEquals(
            "one reorder, for where the row now sits",
            drop(BridgeQueueLane.MANUAL, "m", 2),
            order.released(),
        )
        assertNull("a release sends once", order.released())
        order.apply(listOf(entry("n"), entry("o"), entry("m"), entry("p")), context(total = 0))
        order.reorderReturned(BridgeQueueLane.MANUAL)
        assertEquals(listOf("n", "o", "m", "p"), order.manual.map { it.entryId })
    }

    @Test
    fun aDragThatMovedNothingSendsNothing() {
        val order = QueueOrder()
        order.apply(listOf(entry("m"), entry("n")), context(total = 0))
        order.dragging()
        order.start(BridgeQueueLane.MANUAL, "m", 0)
        assertNull("a step onto a section header moves nothing", order.stepOnto("uphdr"))
        assertNull(order.released())
    }
}
