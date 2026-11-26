package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.BridgeFixtures
import fm.bae.app.R
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidContentUnavailable
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.AvatarFace
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.library.LibraryBrowserState
import fm.bae.app.ui.library.LibraryMoreMenuItems
import fm.bae.app.ui.library.ListPlaceholder
import fm.bae.app.ui.library.PersonButton
import fm.bae.app.ui.library.PersonMenuItems
import fm.bae.app.ui.library.PlaylistRows
import fm.bae.app.ui.library.WhoseMenu
import fm.bae.app.ui.library.historyEmptyText
import org.junit.Test
import uniffi.bae_bridge.BridgeHistoryShown
import uniffi.bae_bridge.BridgePlaylistCover
import uniffi.bae_bridge.BridgeSharing

/**
 * The shots of the person's menu and the ⋮ menu over the library, and of the Playlists and History screens, drawn as
 * the boards draw them.
 */
class PersonMenuBoardShots : BaeBoardShot() {
    /** The library with the person's picture at the end of its header, and [menu] open under the header. */
    @Composable
    private fun LibraryWithMenu(
        end: Dp,
        menu: @Composable () -> Unit,
    ) {
        val state = remember { LibraryBrowserState() }
        Box(modifier = Modifier.fillMaxSize()) {
            PlayingLibraryScreen(state = state)
            OpenMenu(top = MENU_TOP, end = end) { menu() }
        }
    }

    /** The person's menu under their picture. */
    @Test
    fun androidLibraryAvatarMenu() =
        shot("android.library.avatar-menu") {
            ScreenScene {
                LibraryWithMenu(end = ThemeSpace.related) {
                    PersonMenuItems(
                        isAdmin = true,
                        onAddDevice = {},
                        onInvite = {},
                        onPlaylists = {},
                        onHistory = {},
                        onSettings = {},
                    )
                }
            }
        }

    /** The ⋮ menu: the view's sort and Shuffle library, with no Settings. */
    @Test
    fun androidLibraryMoreMenu() =
        shot("android.library.more-menu") {
            ScreenScene {
                LibraryWithMenu(end = 56.dp) {
                    LibraryMoreMenuItems(state = remember { LibraryBrowserState() }, onShuffleLibrary = {})
                }
            }
        }

    /** The playlists, each with its picture, and New playlist under them. */
    @Test
    fun androidPlaylistsList() =
        shot("android.playlists.list") {
            ScreenScene {
                Covers {
                    AndroidScreen(
                        title = stringResource(R.string.library_mode_playlists),
                        onBack = {},
                        backLabel = stringResource(R.string.back_to_library),
                    ) {
                        PlaylistRows(
                            playlists =
                                listOf(
                                    playlist("late-night", "Late night", BridgeSharing.HOUSEHOLD, 12uL, LATE_NIGHT_COVERS),
                                    playlist("kitchen-radio", "Kitchen radio", BridgeSharing.HOUSEHOLD, 40uL, KITCHEN_RADIO_COVERS),
                                    playlist("running", "Running", BridgeSharing.PRIVATE, 18uL, listOf("Small Hours", "Northbound")),
                                    playlist("for-ana", "For Ana", BridgeSharing.PRIVATE, 9uL, listOf("Glass Harbor", "Salt and Cedar")),
                                ),
                            listState = rememberLazyListState(),
                            onSelectPlaylist = {},
                            newPlaylistButton = {
                                AndroidButton(onClick = {}, kind = ButtonKind.TONAL) {
                                    Text(stringResource(R.string.playlist_new))
                                }
                            },
                        )
                    }
                }
            }
        }

    private fun playlist(
        id: String,
        name: String,
        sharing: BridgeSharing,
        tracks: ULong,
        covers: List<String>,
    ) = BridgeFixtures.playlistSummary(id = id, name = name, sharing = sharing).copy(
        // The board draws four covers as a grid and fewer as the first one alone.
        cover =
            if (covers.size == 4) {
                BridgePlaylistCover.Grid(covers = covers.map { PreviewData.imageRef(it) })
            } else {
                BridgePlaylistCover.Single(PreviewData.imageRef(covers.first()))
            },
        trackCount = BridgeFixtures.playlistSummary().trackCount.copy(count = tracks),
    )

    @Composable
    private fun HistoryScene(
        shown: BridgeHistoryShown,
        body: @Composable () -> Unit,
    ) {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            AndroidScreen(
                title = stringResource(R.string.history),
                onBack = {},
                backLabel = stringResource(R.string.back_to_library),
                actions = { WhoseMenu(menu = historyChoice(shown), open = false, onOpen = {}, onChoose = {}) },
            ) {
                Box(modifier = Modifier.fillMaxSize()) { body() }
            }
        }
    }

    /** History with no plays: the line that says so, and the choice of whose plays in the bar. */
    @Test
    fun androidHistoryEmpty() =
        shot("android.history.empty") {
            ScreenScene {
                HistoryScene(BridgeHistoryShown.Everyone) {
                    ListPlaceholder { AndroidContentUnavailable(historyEmptyText(BridgeHistoryShown.Everyone)) }
                }
            }
        }

    /** History when one person is picked and has no plays. */
    @Test
    fun androidHistoryNoneFromPerson() =
        shot("android.history.none-from-person") {
            ScreenScene {
                HistoryScene(BridgeHistoryShown.Person("Ben")) {
                    ListPlaceholder { AndroidContentUnavailable(historyEmptyText(BridgeHistoryShown.Person("Ben"))) }
                }
            }
        }
}

/** The signed-in person's menu button in the library boards. */
@Composable
internal fun BoardLibraryPerson(state: LibraryBrowserState) {
    PersonButton(
        state = state,
        face = AvatarFace(initials = "A", picture = null),
        isAdmin = true,
        onAddDevice = {},
        onInvite = {},
        onPlaylists = {},
        onHistory = {},
        onSettings = {},
    )
}

private val MENU_TOP = 52.dp

/** The covers the boards draw for the playlists list's first two playlists. */
private val LATE_NIGHT_COVERS = listOf("Low Tide Hours", "Copper Sky", "Small Hours", "Paper Lanterns")

private val KITCHEN_RADIO_COVERS = listOf("Copper Sky", "Winter Kitchen", "Glass Harbor", "Field Recordings")
