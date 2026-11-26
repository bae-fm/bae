package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.library.AndroidArtistGroupHeading
import org.junit.Test
import uniffi.bae_bridge.BridgeCountLabel
import uniffi.bae_bridge.BridgeLibraryBrowseSection

/** The component shots of `AndroidArtistGroupHeading`, one for each row of its preview. */
class AndroidArtistGroupHeadingShots : BaeBoardShot() {
    @Test
    fun androidComponentArtistGroupHeadingTop() =
        component("android.component.artist-group-heading.top") {
            InAlbumGrid(pinned = false)
        }

    @Test
    fun androidComponentArtistGroupHeadingPinned() =
        component("android.component.artist-group-heading.pinned") {
            InAlbumGrid(pinned = true)
        }
}

@Composable
private fun InAlbumGrid(pinned: Boolean) {
    Covers {
        Box(modifier = Modifier.padding(ThemeSpace.group)) {
            AndroidArtistGroupHeading(section, pinned = pinned, gridPadding = ThemeSpace.group, onClick = {})
        }
    }
}

private val section =
    BridgeLibraryBrowseSection(
        "artist-1",
        "Mara Quill",
        PreviewData.imageRef("Mara Quill"),
        0uL,
        12uL,
        BridgeCountLabel("core.library.album_count", 12uL),
    )
