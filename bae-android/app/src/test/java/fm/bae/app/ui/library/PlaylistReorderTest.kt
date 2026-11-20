package fm.bae.app.ui.library

import androidx.compose.runtime.mutableStateOf
import fm.bae.app.BridgeFixtures
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeBesideEntry
import uniffi.bae_bridge.BridgeEntryPlace
import uniffi.bae_bridge.BridgePlaylistEntryRow

/** A playlist entry stays where the person moved it until core's write returns. */
class PlaylistReorderTest {
    private val read =
        listOf(
            BridgeFixtures.playlistEntry("a", "Harbor Lights"),
            BridgeFixtures.playlistEntry("b", "Copper Sky"),
            BridgeFixtures.playlistEntry("c", "Night Ferry"),
        )

    private val held = mutableStateOf<List<String>?>(null)
    private val returns = mutableListOf<() -> Unit>()
    private val moves = mutableListOf<Pair<String, BridgeEntryPlace>>()

    private fun reorder(entries: List<BridgePlaylistEntryRow>) =
        PlaylistReorder(
            entries,
            { positionId, place, onReturned ->
                moves += positionId to place
                returns += onReturned
            },
            held,
        )

    private fun PlaylistReorder.order() = shown.map { it.positionId }

    @Test
    fun aMovedEntryStaysUntilItsWriteReturnsWhateverThePlaylistReads() {
        reorder(read).move("a", up = false)
        assertEquals(
            "the page asks core to move the entry one place down",
            listOf("a" to BridgeEntryPlace.Down),
            moves,
        )
        assertEquals(listOf("b", "a", "c"), reorder(read).order())
        assertEquals(
            "the page keeps drawing the moved order when the playlist's rows do not have the move yet",
            listOf("b", "a", "c"),
            reorder(read.reversed()).order(),
        )

        returns.single()()
        assertEquals(
            "the page draws the playlist's rows once the write returns, without waiting for them to match",
            listOf("a", "b", "c"),
            reorder(read).order(),
        )
    }

    @Test
    fun aDragWritesTheEntryItLastPassed() {
        val reorder = reorder(read)
        reorder.start("c")
        reorder.step(onto = "b", down = false)
        reorder.step(onto = "a", down = false)
        assertEquals(
            "the page draws the dragged row where the person would drop it",
            listOf("c", "a", "b"),
            reorder.order(),
        )

        reorder.finish()
        assertEquals(
            "the page asks core to put the dragged entry just before the entry it last passed",
            listOf("c" to BridgeEntryPlace.Beside(BridgeBesideEntry.Before("a"))),
            moves,
        )
        assertEquals(listOf("c", "a", "b"), reorder.order())
    }

    @Test
    fun aDragDownWritesThePlaceAfterTheEntryItPassed() {
        val reorder = reorder(read)
        reorder.start("a")
        reorder.step(onto = "b", down = true)
        reorder.finish()
        assertEquals(listOf("a" to BridgeEntryPlace.Beside(BridgeBesideEntry.After("b"))), moves)
        assertEquals(listOf("b", "a", "c"), reorder.order())
    }

    @Test
    fun aDragNoStepMovedWritesNothing() {
        val reorder = reorder(read)
        reorder.start("b")
        reorder.finish()
        assertEquals(emptyList<Pair<String, BridgeEntryPlace>>(), moves)
        assertEquals(listOf("a", "b", "c"), reorder.order())
    }
}
