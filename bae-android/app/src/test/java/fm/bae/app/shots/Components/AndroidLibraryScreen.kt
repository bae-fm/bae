package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import fm.bae.app.ui.library.AlbumGridLayout
import fm.bae.app.ui.library.AndroidAlbumGrid
import fm.bae.app.ui.library.AndroidLibraryScreen
import fm.bae.app.ui.library.LibraryBrowserState
import org.junit.Test

/** The component shots of `AndroidLibraryScreen`, one for each row of its preview. */
class AndroidLibraryScreenShots : BoardShot() {
    @Test
    fun androidComponentLibraryScreenTop() = shot("android.component.library-screen.top") { ScreenScene { PlayingLibraryScreen() } }
}

/**
 * The library screen on Albums with a track playing in the bar, as the boards draw it behind what opens over it,
 * with [person] at the end of its header.
 */
@Composable
internal fun PlayingLibraryScreen(
    state: LibraryBrowserState = remember { LibraryBrowserState() },
    person: @Composable () -> Unit = {},
) {
    Images {
        AndroidLibraryScreen(
            state = state,
            onShuffleLibrary = {},
            person = person,
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
