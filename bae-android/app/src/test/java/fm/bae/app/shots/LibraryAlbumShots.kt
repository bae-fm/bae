package fm.bae.app.shots

import org.junit.Test

class LibraryAlbumShots : BoardShot() {
    @Test
    fun androidLibraryAlbumsNotOnThisDevice() =
        shot("android.library.albums.not-on-this-device") {
            ScreenScene {
                PlayingLibraryScreen(
                    albums = listOf(boardAlbums.first().copy(primaryReleaseUnplayable = true)) + boardAlbums.drop(1),
                    nowPlayingBar = { Bar(playingNorthbound) },
                )
            }
        }
}
