package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.library.AlbumGridLayout
import fm.bae.app.ui.library.AndroidAlbumGrid
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumAppearance

/** The component shots of `AndroidAlbumGrid`, one for each row of its preview. */
class AndroidAlbumGridShots : BoardShot() {
    @Test
    fun androidComponentAlbumGridDefault() =
        component("android.component.album-grid.default") {
            Covers {
                val rows = previewAlbums
                Box(Modifier.width(380.dp).height(240.dp)) {
                    AndroidAlbumGrid(
                        count = rows.size,
                        layout = AlbumGridLayout.Flat,
                        rowAt = rows::get,
                        gridState = rememberLazyGridState(),
                        onSelectAlbum = {},
                        onSelectArtist = {},
                    )
                }
            }
        }
}

/** An album as the boards name it: its title and the artist it is credited to. */
internal class BoardAlbum(
    val title: String,
    val artist: String,
)

/** The eight albums the boards draw, in order. */
internal val boardAlbumFacts =
    listOf(
        BoardAlbum("Low Tide Hours", "Mara Quill"),
        BoardAlbum("Paper Lanterns", "The Slow Orchards"),
        BoardAlbum("Glass Harbor", "Ines Varga Trio"),
        BoardAlbum("Copper Sky", "Odile Fen"),
        BoardAlbum("Salt and Cedar", "The Slow Orchards"),
        BoardAlbum("Field Recordings", "Mara Quill"),
        BoardAlbum("Winter Kitchen", "Odile Fen"),
        BoardAlbum("Small Hours", "Tomas Brell"),
    )

/** The eight albums the boards' grids draw, in order, each with a cover picture. */
internal val boardAlbums =
    boardAlbumFacts.mapIndexed { index, facts ->
        val id = "a${index + 1}"
        val album = PreviewData.album(id = id, title = facts.title, artistNames = facts.artist, cover = PreviewData.imageRef(facts.title))
        BridgeAlbumAppearance(album, false, album.artistLine)
    }

/** The two albums the album grid's preview holds: the first two of the boards' albums. */
internal val previewAlbums = boardAlbums.take(2)
