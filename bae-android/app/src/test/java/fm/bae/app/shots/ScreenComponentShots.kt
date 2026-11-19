package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidAlbumHeader
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.library.AlbumGridLayout
import fm.bae.app.ui.library.AndroidAlbumGrid
import fm.bae.app.ui.library.AndroidDetailScreen
import fm.bae.app.ui.library.AndroidLibraryScreen
import fm.bae.app.ui.library.LibraryBrowserState
import org.junit.Test

/** The component shots of the two screen frames: a settings screen's, and a library detail page's. */
class ScreenComponentShots : BoardShot() {
    @Test
    fun androidComponentScreenDefault() =
        component("android.component.screen.back", wide = true) {
            AndroidScreen(title = "Library", onBack = {}, modifier = Modifier.height(360.dp)) {
                Text("The screen’s content", modifier = Modifier.padding(ThemeSpace.edge))
            }
        }

    @Test
    fun androidComponentLibraryScreenDefault() = shot("android.component.library-screen.top") { ScreenScene { PlayingLibraryScreen() } }

    @Test
    fun androidComponentDetailScreenDefault() =
        component("android.component.detail-screen.resting", wide = true) {
            Images {
                AndroidDetailScreen(
                    onBack = {},
                    backLabel = stringResource(R.string.back_to_library),
                    nowPlayingBar = { Bar(playing) },
                    modifier = Modifier.height(480.dp),
                    actions = {
                        AndroidIconButton(Icons.Filled.MoreVert, stringResource(R.string.album_options), onClick = {})
                    },
                ) {
                    Box(Modifier.padding(ThemeSpace.edge).weight(1f)) {
                        AndroidAlbumHeader(
                            detail = plainDetail,
                            cover = PreviewData.imageRef(),
                            galleryItemCount = 0u,
                            onShowGallery = {},
                        )
                    }
                }
            }
        }
}

/** The library screen on Albums with a track playing in the bar, as the boards draw it behind what opens over it. */
@Composable
internal fun PlayingLibraryScreen() {
    Images {
        val state = remember { LibraryBrowserState() }
        AndroidLibraryScreen(
            state = state,
            onShuffleLibrary = {},
            onSettings = {},
            syncActivity = {},
            nowPlayingBar = { Bar(playing) },
        ) { modifier ->
            Box(modifier) {
                AndroidAlbumGrid(
                    count = previewAlbums.size,
                    layout = AlbumGridLayout.Flat,
                    rowAt = previewAlbums::get,
                    gridState = state.gridState,
                    onSelectAlbum = {},
                    onSelectArtist = {},
                )
            }
        }
    }
}
