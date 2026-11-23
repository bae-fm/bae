package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.library.AndroidAlbumTile
import org.junit.Test

/** The component shots of `AndroidAlbumTile`, one for each row of its preview. */
class AndroidAlbumTileShots : BoardShot() {
    @Test
    fun androidComponentAlbumTileDefault() = component("android.component.album-tile.default") { Tiles(unplayable = false) }

    @Test
    fun androidComponentAlbumTileUnplayable() = component("android.component.album-tile.unplayable") { Tiles(unplayable = true) }
}

@Composable
private fun Tiles(unplayable: Boolean) =
    Covers {
        Row(
            modifier = Modifier.padding(ThemeSpace.group),
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.group),
        ) {
            AndroidAlbumTile(
                title = "Low Tide Hours",
                line = "Mara Quill",
                cover = PreviewData.imageRef("Low Tide Hours"),
                unplayable = unplayable,
                onClick = {},
                modifier = Modifier.weight(1f),
            )
            AndroidAlbumTile(
                title = "Copper Sky",
                line = "1972",
                cover = null,
                unplayable = false,
                onClick = {},
                modifier = Modifier.weight(1f),
            )
        }
    }
