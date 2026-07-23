package fm.bae.app.ui.library

import androidx.activity.compose.BackHandler
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import fm.bae.app.OpenLibrary
import kotlinx.coroutines.flow.StateFlow
import uniffi.bae_bridge.BridgeLibrary

@Composable
fun LibraryScreen(
    session: OpenLibrary,
    libraries: StateFlow<List<BridgeLibrary>>,
    openSearch: Boolean,
    onSearchOpened: () -> Unit,
    onSwitchLibrary: (BridgeLibrary) -> Unit,
    onLeaveLibrary: () -> Unit,
) {
    val navigator = remember { LibraryNavigator() }
    val browserState = remember { LibraryBrowserState() }
    val stateHolder = rememberSaveableStateHolder()
    val popEntry: () -> Unit = {
        navigator.pop()?.let { stateHolder.removeState(it.key) }
    }
    // The Search shortcut lands on the searchable browser wherever the user last
    // was: unwind any pushed destination (only the browser root hosts the search
    // field), then open search. Keyed on the request so it fires once and re-arms
    // only when a new Search shortcut sets it.
    LaunchedEffect(openSearch) {
        if (openSearch) {
            while (navigator.entries.isNotEmpty()) popEntry()
            browserState.searchOpen = true
            onSearchOpened()
        }
    }
    BackHandler(enabled = navigator.entries.isNotEmpty()) { popEntry() }
    when (val entry = navigator.top) {
        null -> {
            LibraryBrowser(
                session = session,
                state = browserState,
                onSelectAlbum = { navigator.push(LibraryDestination.Album(AlbumTarget(it))) },
                onSelectComposer = { navigator.push(LibraryDestination.Composer(it)) },
                onSelectArtist = { navigator.push(LibraryDestination.Artist(it)) },
                onSelectWork = { navigator.push(LibraryDestination.Work(it)) },
                onSettings = { navigator.push(LibraryDestination.Settings) },
                onDownloads = { navigator.push(LibraryDestination.Downloads) },
            )
        }

        else -> {
            stateHolder.SaveableStateProvider(entry.key) {
                LibraryDestinationScreen(
                    session = session,
                    destination = entry.destination,
                    libraries = libraries,
                    onBack = popEntry,
                    onPush = navigator::push,
                    onSwitchLibrary = onSwitchLibrary,
                    onLeaveLibrary = onLeaveLibrary,
                )
            }
        }
    }
}
