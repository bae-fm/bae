package fm.bae.app.shots

import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.components.PlaylistPicture
import org.junit.Test
import uniffi.bae_bridge.BridgePlaylistCover

/**
 * The component shots of `PlaylistPicture`, one for each row of its preview. A playlist of four or more
 * releases has a grid of four covers; one of fewer has its first release's cover.
 */
class PlaylistPictureShots : BoardShot() {
    @Test
    fun androidComponentPlaylistPictureFourReleases() =
        component("android.component.playlist-picture.four-releases") {
            Picture(BridgePlaylistCover.Grid(listOf("r1", "r2", "r3", "r4").map(PreviewData::imageRef)))
        }

    @Test
    fun androidComponentPlaylistPictureFewerThanFour() =
        component("android.component.playlist-picture.fewer-than-four") { Picture(BridgePlaylistCover.Single(PreviewData.imageRef("r2"))) }

    @Test
    fun androidComponentPlaylistPictureEmpty() =
        component("android.component.playlist-picture.empty") { Picture(BridgePlaylistCover.Single(null)) }
}

@Composable
private fun Picture(cover: BridgePlaylistCover) =
    Covers { PlaylistPicture(cover = cover, cornerRadius = ThemeRadius.artwork, modifier = Modifier.size(120.dp)) }
