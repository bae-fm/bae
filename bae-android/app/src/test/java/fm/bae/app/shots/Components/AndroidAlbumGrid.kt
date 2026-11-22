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

/** The eight albums the boards' grids draw, in order, each with a cover picture. */
internal val boardAlbums =
    listOf(
        "Low Tide Hours" to "Mara Quill",
        "Paper Lanterns" to "The Slow Orchards",
        "Glass Harbor" to "Ines Varga Trio",
        "Copper Sky" to "Odile Fen",
        "Salt and Cedar" to "The Slow Orchards",
        "Field Recordings" to "Mara Quill",
        "Winter Kitchen" to "Odile Fen",
        "Small Hours" to "Tomas Brell",
    ).mapIndexed { index, (title, artist) ->
        val id = "a${index + 1}"
        BridgeAlbumAppearance(PreviewData.album(id = id, title = title, artistNames = artist, cover = PreviewData.imageRef(title)), null)
    }

/** The two albums the album grid's preview holds: the first two of the boards' albums. */
internal val previewAlbums = boardAlbums.take(2)
