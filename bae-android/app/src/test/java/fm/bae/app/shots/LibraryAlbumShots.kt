package fm.bae.app.shots

import androidx.compose.runtime.remember
import fm.bae.app.ui.library.LibraryBrowserState
import org.junit.Test

class LibraryAlbumShots : BoardShot() {
    @Test
    fun androidLibraryAlbumsNotOnThisDevice() =
        shot("android.library.albums.not-on-this-device") {
            ScreenScene {
                val state = remember { LibraryBrowserState() }
                PlayingLibraryScreen(
                    state = state,
                    person = { BoardLibraryPerson(state) },
                    albums = listOf(boardAlbums.first().copy(primaryReleaseUnplayable = true)) + boardAlbums.drop(1),
                    nowPlayingBar = { Bar(playingNorthbound) },
                )
            }
        }
}
