package fm.bae.app.ui.library

import androidx.compose.runtime.mutableStateOf
import fm.bae.app.BridgeFixtures
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgePlaylistEntryRow

/** A playlist's moved entry stays where it was moved until its write returns. */
class PlaylistReorderTest {
    private val read =
        listOf(
            BridgeFixtures.playlistEntry("a", "Harbor Lights"),
            BridgeFixtures.playlistEntry("b", "Copper Sky"),
            BridgeFixtures.playlistEntry("c", "Night Ferry"),
        )

    private val held = mutableStateOf<List<String>?>(null)
    private val returns = mutableListOf<() -> Unit>()
    private val moves = mutableListOf<Pair<String, Int>>()

    private fun reorder(entries: List<BridgePlaylistEntryRow>) =
        PlaylistReorder(
            entries,
            { positionId, to, onReturned ->
                moves += positionId to to
                returns += onReturned
            },
            held,
        )

    private fun PlaylistReorder.order() = shown.map { it.positionId }

    @Test
    fun aMovedEntryStaysUntilItsWriteReturnsWhateverThePlaylistReads() {
        reorder(read).move("a", index = 0, by = 1)
        assertEquals("core is asked to move the entry to its new place", listOf("a" to 1), moves)
        assertEquals(listOf("b", "a", "c"), reorder(read).order())
        assertEquals(
            "a read without the move keeps the moved order",
            listOf("b", "a", "c"),
            reorder(read.reversed()).order(),
        )

        returns.single()()
        assertEquals(
            "the page draws the playlist's read once the write returns, without waiting for it to match",
            listOf("a", "b", "c"),
            reorder(read).order(),
        )
    }

    @Test
    fun aDragWritesTheDraggedEntryAndThePlaceItLandsIn() {
        val reorder = reorder(read)
        reorder.rowHeight = 10f
        reorder.start("c", index = 2)
        reorder.drag(-21f)
        assertEquals("the dragged row is drawn where it would land", listOf("c", "a", "b"), reorder.order())

        reorder.finish()
        assertEquals("core is asked to move the dragged entry to the place it landed", listOf("c" to 0), moves)
    }

    @Test
    fun aDragLetGoWhereItStartedWritesNothing() {
        val reorder = reorder(read)
        reorder.rowHeight = 10f
        reorder.start("b", index = 1)
        reorder.drag(4f)
        reorder.finish()
        assertEquals(emptyList<Pair<String, Int>>(), moves)
    }
}
