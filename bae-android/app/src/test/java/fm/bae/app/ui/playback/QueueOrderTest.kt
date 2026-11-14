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
    fun aDragShowsItsMovesUntilCoresReadShowsTheRowWhereItWasDropped() {
        val order = QueueOrder()
        order.apply(emptyList(), context(total = 6, read = rows("a", "b", "c", "d")))
        order.dragging()
        order.drop("a", "c")
        assertEquals(listOf("b", "c", "a", "d", null, null), order.context.shown())
        assertEquals(2, order.context.indexOf("a"))

        // The drag ends before core answers: the moves stay on screen.
        order.apply(emptyList(), context(total = 6, read = rows("a", "b", "c", "d")))
        assertEquals(listOf("b", "c", "a", "d", null, null), order.context.shown())
        order.released()
        order.moveAnswered(OrderLane.CONTEXT)
        order.apply(emptyList(), context(total = 6, read = rows("a", "c", "b", "d")))
        assertEquals(
            "a read that doesn't draw the row where it was dropped keeps the moves",
            listOf("b", "c", "a", "d", null, null),
            order.context.shown(),
        )

        order.apply(emptyList(), context(total = 6, read = rows("b", "c", "a", "d")))
        assertEquals(listOf("b", "c", "a", "d", null, null), order.context.shown())
        assertEquals("core's read shows the lane again", 2, order.context.indexOf("a"))
        order.context.moveBefore(from = 0, to = 1)
        order.apply(emptyList(), context(total = 6, read = rows("b", "c", "a", "d")))
        assertEquals("nothing holds the lane now", listOf("b", "c", "a", "d", null, null), order.context.shown())
    }

    @Test
    fun aReorderCoreRefusesShowsTheLanesReadAtOnce() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b"), entry("c")), null)
        order.dragging()
        order.drop("a", "c")
        order.apply(listOf(entry("a"), entry("b"), entry("c")), null)
        assertEquals(listOf("b", "c", "a"), order.manual.map { it.entryId })

        order.moveFailed(OrderLane.MANUAL)
        assertEquals("a failed move never stays on screen", listOf("a", "b", "c"), order.manual.map { it.entryId })
    }

    @Test
    fun aReorderCoreAnswersFalseEndsTheHoldBeforeAnyRowArrives() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b")), null)
        order.dragging()
        order.drop("a", "b")
        order.released()
        order.apply(listOf(entry("a"), entry("b")), null)
        assertEquals("the hold waits for core's answer", listOf("b", "a"), order.manual.map { it.entryId })

        order.moveFailed(OrderLane.MANUAL)
        assertEquals(listOf("a", "b"), order.manual.map { it.entryId })
        order.apply(listOf(entry("a"), entry("b")), null)
        assertEquals("nothing waits on a later reorder", listOf("a", "b"), order.manual.map { it.entryId })
    }

    @Test
    fun anEarlierDragsFailureLeavesALaterDragsHold() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b"), entry("c")), null)
        order.dragging()
        order.drop("a", "b")
        order.released()
        order.dragging()
        order.drop("c", "a")
        order.released()
        order.apply(listOf(entry("a"), entry("b"), entry("c")), null)

        order.moveFailed(OrderLane.MANUAL)
        assertEquals("the second reorder is still unanswered", listOf("b", "c", "a"), order.manual.map { it.entryId })
        order.moveFailed(OrderLane.MANUAL)
        assertEquals(listOf("a", "b", "c"), order.manual.map { it.entryId })
    }

    @Test
    fun aReadThatShowsTheMoveBeforeTheAnswerEndsTheHoldAtTheAnswer() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b")), null)
        order.dragging()
        order.drop("a", "b")
        order.released()
        order.apply(listOf(entry("b"), entry("a")), null)
        order.moveAnswered(OrderLane.MANUAL)
        order.apply(listOf(entry("b"), entry("a")), null)
        order.apply(listOf(entry("a"), entry("b")), null)
        assertEquals("core's later read wins once the row arrived", listOf("a", "b"), order.manual.map { it.entryId })
    }

    @Test
    fun aReadThatShowsTheMoveAfterTheAnswerEndsTheHold() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b")), null)
        order.dragging()
        order.drop("a", "b")
        order.released()
        order.apply(listOf(entry("b"), entry("a")), null)
        assertEquals("the hold waits for core's answer", listOf("b", "a"), order.manual.map { it.entryId })

        order.moveAnswered(OrderLane.MANUAL)
        order.apply(listOf(entry("b"), entry("a"), entry("c")), null)
        assertEquals(listOf("b", "a", "c"), order.manual.map { it.entryId })
    }

    @Test
    fun aLaneOfAnotherLengthEndsTheHoldWhateverItShows() {
        val order = QueueOrder()
        order.apply(listOf(entry("a"), entry("b")), null)
        order.dragging()
        order.drop("a", "b")
        order.released()
        order.moveAnswered(OrderLane.MANUAL)
        order.apply(listOf(entry("c")), null)
        assertEquals(listOf("c"), order.manual.map { it.entryId })
    }

    @Test
    fun eachReadOfTheTailShowsTheRowsItReadAndNoMore() {
        val order = QueueOrder()
        order.apply(emptyList(), context(total = 4, read = rows("a", "b")))
        order.apply(emptyList(), context(total = 4, read = rows("a", "b") + mapOf(3 to "d")))
        assertEquals("a scroll fills the rows it read", listOf("a", "b", null, "d"), order.context.shown())

        order.apply(emptyList(), context(total = 3, read = rows("b", "d")))
        assertEquals(listOf("b", "d", null), order.context.shown())
        assertNull(order.context.indexOf("a"))
    }

    @Test
    fun aHoldOverOneLaneLeavesTheOtherFollowingCore() {
        val order = QueueOrder()
        order.apply(listOf(entry("m")), context(total = 2, read = rows("a", "b")))
        order.dragging()
        order.drop("a", "b")
        order.apply(listOf(entry("n")), context(total = 2, read = rows("a", "b")))
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
        )
        assertEquals(LaneDrop(OrderLane.MANUAL, "m", LanePlace.End, 1), order.drop("m", "n"))
        assertEquals(listOf("n", "m"), order.manual.map { it.entryId })
        assertEquals(LaneDrop(OrderLane.CONTEXT, "a", LanePlace.Before("c"), 1), order.drop("a", "b"))
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

    @Test
    fun aDragSendsCoreOneReorderAtItsReleaseHoldingTheLaneThroughIt() {
        val order = QueueOrder()
        order.apply(listOf(entry("m"), entry("n"), entry("o"), entry("p")), context(total = 0))
        order.dragging()
        // The held row passes "n" then "o": the rows move on screen, and nothing is sent.
        order.drop("m", "n")
        order.drop("m", "o")
        assertEquals(listOf("n", "o", "m", "p"), order.manual.map { it.entryId })
        // The drag's end re-seeds the lanes from core's read, which the lane's hold keeps from undoing the moves.
        order.apply(listOf(entry("m"), entry("n"), entry("o"), entry("p")), context(total = 0))
        assertEquals("the moves stay until core answers", listOf("n", "o", "m", "p"), order.manual.map { it.entryId })

        assertEquals(
            "one reorder, for where the row now sits",
            LaneDrop(OrderLane.MANUAL, "m", LanePlace.Before("p"), 2),
            order.released(),
        )
        assertNull("a release sends once", order.released())
        order.moveAnswered(OrderLane.MANUAL)
        order.apply(listOf(entry("n"), entry("o"), entry("m"), entry("p")), context(total = 0))
        assertEquals(listOf("n", "o", "m", "p"), order.manual.map { it.entryId })
    }

    @Test
    fun aDragThatMovedNothingSendsNothing() {
        val order = QueueOrder()
        order.apply(listOf(entry("m"), entry("n")), context(total = 0))
        order.dragging()
        assertNull(order.drop("m", "header"))
        assertNull(order.released())
    }
}
