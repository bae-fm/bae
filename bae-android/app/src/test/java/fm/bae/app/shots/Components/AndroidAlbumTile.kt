package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.library.AndroidAlbumTile
import org.junit.Test

/** The component shots of `AndroidAlbumTile`, one for each row of its preview. */
class AndroidAlbumTileShots : BoardShot() {
    @Test
    fun androidComponentAlbumTileDefault() =
        component("android.component.album-tile.default") {
            Images {
                Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.edge)) {
                    Box(Modifier.width(160.dp)) {
                        AndroidAlbumTile("Low Tide Hours", "Mara Quill", PreviewData.imageRef(), onClick = {})
                    }
                    Box(Modifier.width(160.dp)) { AndroidAlbumTile("Copper Sky", "1972", null, onClick = {}) }
                }
            }
        }
}
