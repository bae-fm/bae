package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidAlbumHeader
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.library.AndroidDetailScreen
import org.junit.Test

/** The component shots of the two screen frames: a settings screen's, and a library detail page's. */
class ScreenComponentShots : BoardShot() {
    @Test
    fun androidComponentScreenDefault() =
        component("android.component.screen.default", wide = true) {
            AndroidScreen(title = "Transfers", onBack = {}, modifier = Modifier.height(280.dp)) {
                Text("The screen's content", modifier = Modifier.padding(ThemeSpace.edge))
            }
        }

    @Test
    fun androidComponentDetailScreenDefault() =
        component("android.component.detail-screen.default", wide = true) {
            Images {
                AndroidDetailScreen(
                    onBack = {},
                    nowPlayingBar = { Bar(playing) },
                    modifier = Modifier.height(480.dp),
                    actions = { IconButton(onClick = {}) { Icon(Icons.Filled.MoreVert, contentDescription = "More") } },
                ) {
                    Box(Modifier.padding(ThemeSpace.edge).weight(1f)) {
                        AndroidAlbumHeader(
                            detail = PreviewData.albumDetail(PreviewData.album(title = "Low Tide Hours", artistNames = "Mara Quill")),
                            selectedRelease = PreviewData.release(),
                            cover = PreviewData.imageRef(),
                            galleryItems = emptyList(),
                            onShowGallery = {},
                        )
                    }
                }
            }
        }
}
