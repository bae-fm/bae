package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.library.AndroidArtistGroupHeading
import org.junit.Test
import uniffi.bae_bridge.BridgeLibraryBrowseSection

/** The component shots of `AndroidArtistGroupHeading`, one for each row of its preview. */
class AndroidArtistGroupHeadingShots : BoardShot() {
    @Test
    fun androidComponentArtistGroupHeadingTop() =
        component("android.component.artist-group-heading.top", wide = true) {
            Images { AndroidArtistGroupHeading(section, pinned = false, gridPadding = 16.dp, onClick = {}) }
        }

    @Test
    fun androidComponentArtistGroupHeadingPinned() =
        component("android.component.artist-group-heading.pinned", wide = true) {
            Images { AndroidArtistGroupHeading(section, pinned = true, gridPadding = 16.dp, onClick = {}) }
        }
}

private val section = BridgeLibraryBrowseSection("artist-1", "Mara Quill", null, 0uL, 12uL)
