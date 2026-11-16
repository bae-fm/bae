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

    private fun reorder(entries: List<BridgePlaylistEntryRow>) = PlaylistReorder(entries, { _, onReturned -> returns += onReturned }, held)

    private fun PlaylistReorder.order() = shown.map { it.positionId }

    @Test
    fun aMovedEntryStaysUntilItsWriteReturnsWhateverThePlaylistReads() {
        reorder(read).move("a", 1)
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
}
