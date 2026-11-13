package fm.bae.app.ui.playback

import fm.bae.app.creditLineNamed
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

    @Test
    fun theLaneIsSizedToTheWholeTailAndShowsEachReadRow() {
        val lane = ContextLaneOrder()
        lane.seed(context(total = 5, read = rows("a", "b") + mapOf(4 to "e")))
        assertEquals(listOf("a", "b", null, null, "e"), lane.shown())
    }

    @Test
    fun aDragShowsItsMovesUntilCoresReadShowsTheVersionItsReorderMade() {
        val order = QueueOrder()
        order.apply(emptyList(), context(total = 6, read = rows("a", "b", "c", "d")), manualVersion = 1u, contextVersion = 1u)
        order.dragging()
        order.context.moveBefore(from = 0, to = 2)
        order.moveSent(OrderLane.CONTEXT)
        assertEquals(listOf("b", "c", "a", "d", null, null), order.context.shown())
        assertEquals(2, order.context.indexOf("a"))
        order.context.moveBefore(from = 3, to = 1)
        order.moveSent(OrderLane.CONTEXT)
        assertEquals(listOf("b", "d", "c", "a", null, null), order.context.shown())
        assertEquals(3, order.context.indexOf("a"))

        // The drag ends before core answers: the moves stay on screen.
        order.apply(emptyList(), context(total = 6, read = rows("a", "b", "c", "d")), manualVersion = 1u, contextVersion = 1u)
        assertEquals(listOf("b", "d", "c", "a", null, null), order.context.shown())
        order.moveAnswered(OrderLane.CONTEXT, version = 2u)
        order.moveAnswered(OrderLane.CONTEXT, version = 3u)
        order.apply(emptyList(), context(total = 6, read = rows("b", "a", "c", "d")), manualVersion = 1u, contextVersion = 2u)
        assertEquals(
            "a read before the last move's version keeps the moves",
            listOf("b", "d", "c", "a", null, null),
            order.context.shown(),
        )

        order.apply(emptyList(), context(total = 6, read = rows("b", "d", "c", "a")), manualVersion = 1u, contextVersion = 3u)
        assertEquals(listOf("b", "d", "c", "a", null, null), order.context.shown())
        assertEquals("core's read shows the lane again", 3, order.context.indexOf("a"))
        order.context.moveBefore(from = 0, to = 1)
        order.apply(emptyList(), context(total = 6, read = rows("b", "d", "c", "a")), manualVersion = 1u, contextVersion = 4u)
        assertEquals("nothing holds the lane now", listOf("b", "d", "c", "a", null, null), order.context.shown())
    }

    @Test
    fun aReorderCoreRefusesShowsTheLanesReadAtOnce() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b"), entry("c")), null, manualVersion = 5u, contextVersion = 0u)
        order.dragging()
        val moved = order.manual.removeAt(0)
        order.manual.add(2, moved)
        order.moveSent(OrderLane.MANUAL)
        order.apply(listOf(entry("a"), entry("b"), entry("c")), null, manualVersion = 5u, contextVersion = 0u)
        assertEquals(listOf("b", "c", "a"), order.manual.map { it.entryId })

        order.moveFailed(OrderLane.MANUAL)
        assertEquals("a failed move never stays on screen", listOf("a", "b", "c"), order.manual.map { it.entryId })
    }

    @Test
    fun anAnswerTheReadAlreadyShowsEndsTheHold() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b")), null, manualVersion = 1u, contextVersion = 0u)
        order.dragging()
        val moved = order.manual.removeAt(0)
        order.manual.add(1, moved)
        order.moveSent(OrderLane.MANUAL)
        order.apply(listOf(entry("b"), entry("a")), null, manualVersion = 2u, contextVersion = 0u)
        order.apply(listOf(entry("b"), entry("a"), entry("c")), null, manualVersion = 3u, contextVersion = 0u)
        assertEquals("the hold waits for core's answer", listOf("b", "a"), order.manual.map { it.entryId })

        order.moveAnswered(OrderLane.MANUAL, version = 2u)
        assertEquals(listOf("b", "a", "c"), order.manual.map { it.entryId })
    }

    @Test
    fun moreOfAnUnchangedTailFillsOnlyItsRowsAndAChangeReseedsIt() {
        val order = QueueOrder()
        order.apply(emptyList(), context(total = 4, read = rows("a", "b")), manualVersion = 1u, contextVersion = 1u)
        order.apply(
            emptyList(),
            context(total = 4, read = rows("a", "b") + mapOf(3 to "d")),
            manualVersion = 1u,
            contextVersion = 1u,
        )
        assertEquals("a scroll fills the rows it read", listOf("a", "b", null, "d"), order.context.shown())

        order.apply(emptyList(), context(total = 3, read = rows("b", "d")), manualVersion = 1u, contextVersion = 2u)
        assertEquals(listOf("b", "d", null), order.context.shown())
        assertNull(order.context.indexOf("a"))
    }

    @Test
    fun aHoldOverOneLaneLeavesTheOtherFollowingCore() {
        val order = QueueOrder()
        order.apply(listOf(entry("m")), context(total = 2, read = rows("a", "b")), manualVersion = 1u, contextVersion = 1u)
        order.dragging()
        order.context.moveBefore(from = 0, to = 1)
        order.moveSent(OrderLane.CONTEXT)
        order.apply(listOf(entry("n")), context(total = 2, read = rows("a", "b")), manualVersion = 2u, contextVersion = 1u)
        assertEquals(listOf("n"), order.manual.map { it.entryId })
        assertEquals("the context lane keeps its moves", listOf("b", "a"), order.context.shown())
    }

    @Test
    fun aDropJustBeforeARowNotReadYetMovesNothing() {
        val lane = ContextLaneOrder()
        lane.seed(context(total = 5, read = rows("a", "b", "c")))
        assertNull("core can't be told the unread row it sits before", lane.moveBefore(from = 0, to = 2))
        assertEquals(listOf("a", "b", "c", null, null), lane.shown())

        assertEquals(LanePlace.Before("c"), lane.moveBefore(from = 0, to = 1))
        assertEquals(listOf("b", "a", "c", null, null), lane.shown())
    }

    @Test
    fun aDropTellsCoreTheRowItNowSitsBeforeInItsLane() {
        val order = QueueOrder()
        order.apply(
            listOf(entry("m"), entry("n")),
            context(total = 4, read = rows("a", "b", "c")),
            manualVersion = 1u,
            contextVersion = 1u,
        )
        assertEquals(LaneDrop(OrderLane.MANUAL, "m", LanePlace.End), order.drop("m", "n"))
        assertEquals(listOf("n", "m"), order.manual.map { it.entryId })
        assertEquals(LaneDrop(OrderLane.CONTEXT, "a", LanePlace.Before("c")), order.drop("a", "b"))
        assertEquals(listOf("b", "a", "c", null), order.context.shown())
        assertNull("a drop just before an unread row moves nothing", order.drop("b", "c"))
        assertEquals(listOf("b", "a", "c", null), order.context.shown())
        assertNull("a step across the lanes moves nothing", order.drop("m", "a"))
    }

    @Test
    fun aDropAfterTheLastRowSitsAtTheLanesEnd() {
        val lane = ContextLaneOrder()
        lane.seed(context(total = 3, read = rows("a", "b", "c")))
        assertEquals(LanePlace.End, lane.moveBefore(from = 0, to = 2))
        assertEquals(listOf("b", "c", "a"), lane.shown())
    }
}
