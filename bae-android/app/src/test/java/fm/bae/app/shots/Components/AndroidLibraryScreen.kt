package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.library.AlbumGridLayout
import fm.bae.app.ui.library.AndroidAlbumGrid
import fm.bae.app.ui.library.AndroidLibraryScreen
import fm.bae.app.ui.library.LibraryBrowserState
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumAppearance

/** The component shots of `AndroidLibraryScreen`, one for each row of its preview. */
class AndroidLibraryScreenShots : BoardShot() {
    @Test
    fun androidComponentLibraryScreenTop() = component("android.component.library-screen.top") { LibraryScreenPreview() }

    /** The grid scrolled past its first row, which folds the header into its slim bar. */
    @Test
    fun androidComponentLibraryScreenScrolled() =
        component("android.component.library-screen.scrolled") {
            val state = remember { LibraryBrowserState().apply { gridState.requestScrollToItem(1) } }
            LibraryScreenPreview(state = state)
        }

    /** Search open with words typed: the search bar takes the header's place. */
    @Test
    fun androidComponentLibraryScreenSearching() =
        component("android.component.library-screen.searching") {
            val state =
                remember {
                    LibraryBrowserState().apply {
                        searchOpen = true
                        searchQuery = "harbor"
                    }
                }
            LibraryScreenPreview(state = state)
        }

    /** Nothing has played on this phone, so the screen has no now playing bar. */
    @Test
    fun androidComponentLibraryScreenNothingPlayed() =
        component("android.component.library-screen.nothing-played") { LibraryScreenPreview(nowPlayingBar = {}) }

    @Test
    fun androidComponentLibraryScreenBarError() =
        component("android.component.library-screen.bar-error") {
            LibraryScreenPreview(nowPlayingBar = { Bar(playing, failure = DisplayError("Could not read what is playing.")) })
        }

    @Test
    fun androidComponentLibraryScreenSyncMoving() =
        component("android.component.library-screen.sync-moving") {
            LibraryScreenPreview(syncActivity = { ActivityButton(PreviewData.syncActivity(needsAttention = false)) })
        }

    @Test
    fun androidComponentLibraryScreenSyncAttention() =
        component("android.component.library-screen.sync-attention") {
            LibraryScreenPreview(syncActivity = {
                ActivityButton(idleActivity.copy(failure = PreviewData.syncActivity(needsAttention = true).failure))
            })
        }

    @Test
    fun androidComponentLibraryScreenSyncPaused() =
        component("android.component.library-screen.sync-paused") {
            LibraryScreenPreview(syncActivity = { ActivityButton(idleActivity.copy(held = true)) })
        }

    @Test
    fun androidComponentLibraryScreenTrackNoCover() =
        component("android.component.library-screen.track-no-cover") { LibraryScreenPreview(nowPlayingBar = { Bar(withoutCover) }) }

    @Test
    fun androidComponentLibraryScreenWaiting() =
        component("android.component.library-screen.waiting") { LibraryScreenPreview(nowPlayingBar = { Bar(playingWaitingOnStorage) }) }
}

/** The library shell with the preview's label in its content slot. */
@Composable
private fun LibraryScreenPreview(
    state: LibraryBrowserState = remember { LibraryBrowserState() },
    nowPlayingBar: @Composable () -> Unit = { Bar(playing) },
    syncActivity: @Composable () -> Unit = {},
) {
    AndroidLibraryScreen(
        state = state,
        onShuffleLibrary = {},
        person = { BoardLibraryPerson(state) },
        syncActivity = syncActivity,
        nowPlayingBar = nowPlayingBar,
    ) { modifier ->
        Box(modifier) {
            PreviewSlot(if (state.searchOpen) "Results" else stringResource(R.string.library_mode_albums))
        }
    }
}

/** The preview's label inside its dashed border. */
@Composable
internal fun PreviewSlot(label: String) {
    val outline = BaeTheme.colors.hairlineStrong
    Text(
        text = label,
        style = ThemeText.body.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier =
            Modifier
                .fillMaxWidth()
                .drawWithCache {
                    val width = ThemeSpace.hairline.toPx()
                    val inset = width / 2
                    val radius = CornerRadius(ThemeRadius.card.toPx() - inset)
                    val stroke = Stroke(width, pathEffect = PathEffect.dashPathEffect(floatArrayOf(3 * width, 2 * width)))
                    onDrawBehind {
                        drawRoundRect(
                            color = outline,
                            topLeft = Offset(inset, inset),
                            size = Size(size.width - width, size.height - width),
                            cornerRadius = radius,
                            style = stroke,
                        )
                    }
                }.padding(ThemeSpace.group + ThemeSpace.hairline),
    )
}

/**
 * The library screen on Albums, showing [albums], with [nowPlayingBar] at its foot (a track playing, unless a shot says
 * otherwise), as the boards draw it behind what opens over it, with the person's menu at the end of its header.
 */
@Composable
internal fun PlayingLibraryScreen(
    state: LibraryBrowserState = remember { LibraryBrowserState() },
    albums: List<BridgeAlbumAppearance> = boardAlbums,
    nowPlayingBar: @Composable () -> Unit = { Bar(playing) },
) {
    Covers {
        AndroidLibraryScreen(
            state = state,
            onShuffleLibrary = {},
            person = { BoardLibraryPerson(state) },
            syncActivity = {},
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
