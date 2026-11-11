package fm.bae.app.ui.library

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import fm.bae.app.LibraryListing
import fm.bae.app.OpenLibrary
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.ui.AppProblem
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.sync.activityProblems
import kotlinx.coroutines.flow.StateFlow

@Composable
internal fun LibraryScreen(
    session: OpenLibrary,
    libraries: StateFlow<LibraryListing>,
    launchProblems: List<AppProblem>,
    openSearch: Boolean,
    onSearchOpened: () -> Unit,
    actions: SessionActions,
) {
    val navigator = remember { LibraryNavigator() }
    val browserState = remember { LibraryBrowserState() }
    val stateHolder = rememberSaveableStateHolder()
    ActivityDismissalsPruned(session.syncActivityStore, launchProblems, browserState)
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
            AndroidLibraryScreenContainer(
                session = session,
                state = browserState,
                launchProblems = launchProblems,
                onSelectAlbum = { navigator.push(LibraryDestination.Album(AlbumTarget(it))) },
                onSelectComposer = { navigator.push(LibraryDestination.Composer(it)) },
                onSelectArtist = { navigator.push(LibraryDestination.Artist(it)) },
                onSelectWork = { navigator.push(LibraryDestination.Work(it)) },
                onSettings = { navigator.push(LibraryDestination.Settings) },
                onDownloads = { navigator.push(LibraryDestination.Downloads) },
                onSyncStatus = { navigator.push(LibraryDestination.Sync) },
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
                    actions = actions,
                )
            }
        }
    }
}

/**
 * Keeps the Activity sheet's dismissals to the problems still listed, so a problem that leaves the
 * list and comes back shows again. It reads the store itself, so a change to the activity
 * recomposes this alone.
 */
@Composable
private fun ActivityDismissalsPruned(
    store: SyncActivityStore,
    app: List<AppProblem>,
    state: LibraryBrowserState,
) {
    val activity by store.activity.collectAsState()
    val error by store.error.collectAsState()
    val listed = activityProblems(app, activity, error, emptySet()).listed
    LaunchedEffect(listed) { state.activityDismissed = state.activityDismissed.intersect(listed.toSet()) }
}
