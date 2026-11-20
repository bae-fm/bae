package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidAlbumHeader
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.AndroidTextField
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.library.AlbumGridLayout
import fm.bae.app.ui.library.AndroidAlbumGrid
import fm.bae.app.ui.library.AndroidDetailScreen
import fm.bae.app.ui.library.AndroidLibraryScreen
import fm.bae.app.ui.library.LibraryBrowserState
import fm.bae.app.ui.library.WhoseMenu
import org.junit.Test
import uniffi.bae_bridge.BridgeHistoryShown

/** The component shots of the screen frames: a settings screen's, with its dialog and keyboard, the library screen's, and a library detail page's. */
class ScreenComponentShots : BoardShot() {
    @Test
    fun androidComponentScreenBack() =
        component("android.component.screen.back", wide = true) {
            AndroidScreen(title = "Library", onBack = {}, modifier = Modifier.height(360.dp)) {
                Text("The screen’s content", modifier = Modifier.padding(ThemeSpace.edge))
            }
        }

    @Test
    fun androidComponentScreenMenu() =
        component("android.component.screen.menu", wide = true) {
            AndroidScreen(
                title = "History",
                onBack = {},
                modifier = Modifier.height(360.dp),
                actions = { WhoseMenu(menu = historyChoice(BridgeHistoryShown.Everyone), open = false, onOpen = {}, onChoose = {}) },
            ) {
                Text("The screen’s content", modifier = Modifier.padding(ThemeSpace.edge))
            }
        }

    @Test
    fun androidComponentScreenDialog() =
        shot("android.component.screen.dialog") {
            InPlaceDialogScene(
                behind = {
                    AndroidScreen(title = "Storage", onBack = {}) {
                        Text("The screen’s content", modifier = Modifier.padding(ThemeSpace.edge))
                    }
                },
            ) {
                DialogParts(
                    title = "Remove library from this device?",
                    message = "The library will remain in the cloud and on your other devices.",
                    action = DialogAction("Remove", {}, destructive = true),
                    dismiss = DialogAction("Cancel", {}),
                )
            }
        }

    @Test
    fun androidComponentScreenKeyboard() =
        shot("android.component.screen.keyboard") {
            KeyboardScene {
                AndroidScreen(title = "Name", onBack = {}) {
                    val focus = remember { FocusRequester() }
                    AndroidTextField(
                        value = "Home",
                        onValueChange = {},
                        label = "Library name",
                        modifier = Modifier.padding(ThemeSpace.edge).focusRequester(focus),
                    )
                    LaunchedEffect(Unit) { focus.requestFocus() }
                }
            }
        }

    @Test
    fun androidComponentLibraryScreenTop() = shot("android.component.library-screen.top") { ScreenScene { PlayingLibraryScreen() } }

    @Test
    fun androidComponentDetailScreenResting() =
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
