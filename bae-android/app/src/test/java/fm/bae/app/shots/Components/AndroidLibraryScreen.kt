package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import fm.bae.app.DisplayError
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.library.AlbumGridLayout
import fm.bae.app.ui.library.AndroidAlbumGrid
import fm.bae.app.ui.library.AndroidLibraryScreen
import fm.bae.app.ui.library.LibraryBrowserState
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumAppearance

/** The component shots of `AndroidLibraryScreen`, one for each row of its preview. */
class AndroidLibraryScreenShots : BoardShot() {
    @Test
    fun androidComponentLibraryScreenTop() = shot("android.component.library-screen.top") { ScreenScene { PlayingLibraryScreen() } }

    /** The grid scrolled past its first rows, which folds the header into its slim bar. */
    @Test
    fun androidComponentLibraryScreenScrolled() =
        shot("android.component.library-screen.scrolled") {
            ScreenScene {
                val state = remember { LibraryBrowserState() }
                LaunchedEffect(Unit) { state.gridState.scrollToItem(SCROLLED_TO) }
                PlayingLibraryScreen(state = state, albums = manyAlbums)
            }
        }

    /** Search open with words typed: the search bar takes the header's place. */
    @Test
    fun androidComponentLibraryScreenSearching() =
        shot("android.component.library-screen.searching") {
            ScreenScene {
                PlayingLibraryScreen(
                    state =
                        remember {
                            LibraryBrowserState().apply {
                                searchOpen = true
                                searchQuery = "harbor"
                            }
                        },
                )
            }
        }

    /** Nothing has played on this phone, so the screen has no now playing bar. */
    @Test
    fun androidComponentLibraryScreenNothingPlayed() =
        shot("android.component.library-screen.nothing-played") { ScreenScene { PlayingLibraryScreen(nowPlayingBar = {}) } }

    @Test
    fun androidComponentLibraryScreenBarError() =
        shot("android.component.library-screen.bar-error") {
            ScreenScene {
                PlayingLibraryScreen(
                    nowPlayingBar = { Bar(playing, failure = DisplayError("Could not read what is playing.")) },
                )
            }
        }

    @Test
    fun androidComponentLibraryScreenSyncMoving() =
        shot("android.component.library-screen.sync-moving") {
            ScreenScene { PlayingLibraryScreen(syncActivity = { ActivityButton(PreviewData.syncActivity(needsAttention = false)) }) }
        }

    @Test
    fun androidComponentLibraryScreenSyncAttention() =
        shot("android.component.library-screen.sync-attention") {
            ScreenScene {
                PlayingLibraryScreen(syncActivity = {
                    ActivityButton(idleActivity.copy(failure = PreviewData.syncActivity(needsAttention = true).failure))
                })
            }
        }

    @Test
    fun androidComponentLibraryScreenSyncPaused() =
        shot("android.component.library-screen.sync-paused") {
            ScreenScene { PlayingLibraryScreen(syncActivity = { ActivityButton(idleActivity.copy(held = true)) }) }
        }

    @Test
    fun androidComponentLibraryScreenTrackNoCover() =
        shot("android.component.library-screen.track-no-cover") { ScreenScene { PlayingLibraryScreen() } }

    @Test
    fun androidComponentLibraryScreenWaiting() = shot("android.component.library-screen.waiting") { ScreenScene { PlayingLibraryScreen() } }
}

/**
 * The library screen on Albums, showing [albums], with [nowPlayingBar] at its foot (a track playing, unless a shot says
 * otherwise), as the boards draw it behind what opens over it, with [person] at the end of its header.
 */
@Composable
internal fun PlayingLibraryScreen(
    state: LibraryBrowserState = remember { LibraryBrowserState() },
    person: @Composable () -> Unit = {},
    albums: List<BridgeAlbumAppearance> = previewAlbums,
    nowPlayingBar: @Composable () -> Unit = { Bar(playing) },
    syncActivity: @Composable () -> Unit = {},
) {
    Images {
        AndroidLibraryScreen(
            state = state,
            onShuffleLibrary = {},
            person = person,
            syncActivity = syncActivity,
            nowPlayingBar = nowPlayingBar,
        ) { modifier ->
            Box(modifier) {
                AndroidAlbumGrid(
                    count = albums.size,
                    layout = AlbumGridLayout.Flat,
                    rowAt = albums::get,
                    gridState = state.gridState,
                    onSelectAlbum = {},
                    onSelectArtist = {},
                )
            }
        }
    }
}

/** Enough albums to fill the grid past one screen, so it can scroll. */
private val manyAlbums = List(MANY_ALBUMS) { index -> previewAlbums[index % previewAlbums.size] }

private const val MANY_ALBUMS = 24

/** The item the scrolled shot puts at the top: past the first rows, so the header has folded. */
private const val SCROLLED_TO = 6
