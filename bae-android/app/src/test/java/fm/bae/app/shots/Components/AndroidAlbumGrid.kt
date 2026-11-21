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
            Images {
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

/** The two albums the album grid's and library screen's previews hold. */
internal val previewAlbums =
    listOf(
        BridgeAlbumAppearance(PreviewData.album(id = "a1", title = "Low Tide Hours", artistNames = "Mara Quill"), null),
        BridgeAlbumAppearance(PreviewData.album(id = "a2", title = "Paper Lanterns", artistNames = "The Slow Orchards"), null),
    )
