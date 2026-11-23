package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.remember
import fm.bae.app.DisplayError
import fm.bae.app.data.PageError
import fm.bae.app.ui.library.AndroidLibraryScreen
import fm.bae.app.ui.library.LibraryBrowserState
import fm.bae.app.ui.library.LibraryReadFailed
import org.junit.Test

/** The shots of a library screen that fails as a whole. */
class LibraryFailedShots : BoardShot() {
    /** The albums view when bae cannot read its list: the library's header over the centred notice. */
    @Test
    fun androidLibraryAlbumsFailed() =
        shot("android.library.albums.failed") {
            ScreenScene {
                val state = remember { LibraryBrowserState() }
                val failure =
                    PageError(
                        DisplayError("Could not read your library", "SqliteFailure(DatabaseCorrupt)\ndatabase disk image is malformed"),
                        onRetry = {},
                    )
                AndroidLibraryScreen(
                    state = state,
                    onShuffleLibrary = {},
                    person = { BoardLibraryPerson(state) },
                    syncActivity = {},
                    nowPlayingBar = {},
                ) { modifier -> Box(modifier) { LibraryReadFailed(failure) } }
            }
        }
}
