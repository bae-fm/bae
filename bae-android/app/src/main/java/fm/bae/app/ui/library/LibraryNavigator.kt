package fm.bae.app.ui.library

import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateListOf
import fm.bae.app.OpenLibrary
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.albumdetail.AlbumDetailScreen
import fm.bae.app.ui.downloads.DownloadsScreen
import fm.bae.app.ui.settings.MembersScreen
import fm.bae.app.ui.settings.SettingsScreen
import fm.bae.app.ui.settings.StorageSetupScreen
import fm.bae.app.ui.settings.SyncPendingScreen
import kotlinx.coroutines.flow.StateFlow
import uniffi.bae_bridge.BridgeLibrary

internal data class AlbumTarget(
    val albumId: String,
    val initialReleaseId: String? = null,
)

internal sealed interface LibraryDestination {
    data class Album(
        val target: AlbumTarget,
    ) : LibraryDestination

    data class Work(
        val workId: String,
    ) : LibraryDestination

    data class Composer(
        val artistId: String,
    ) : LibraryDestination

    data class Artist(
        val artistId: String,
    ) : LibraryDestination

    /** Settings and the screens it opens. */
    sealed interface Setting : LibraryDestination

    data object Members : Setting

    data object Settings : Setting

    /** What sync waits on, each with its action. */
    data object SyncPending : Setting

    /** Choosing and setting up a cloud home for a library without one. */
    data object StorageSetup : Setting

    data object Downloads : LibraryDestination
}

/**
 * One pushed level of the library navigation stack. [key] is unique per push
 * for the composition lifetime and keys the entry's saved UI state (scroll
 * positions) in the [androidx.compose.runtime.saveable.SaveableStateHolder],
 * so revisiting the same destination twice never resurrects stale state.
 */
internal data class LibraryStackEntry(
    val key: Long,
    val destination: LibraryDestination,
)

/**
 * In-memory back stack over the library browser. An empty stack shows the
 * browser; pushing shows the new top; popping returns to the previous level.
 * Backed by snapshot state so composition reacts to changes. The stack lives
 * only as long as the open-library composition, like all other UI state here.
 */
internal class LibraryNavigator {
    private var nextKey = 0L
    val entries = mutableStateListOf<LibraryStackEntry>()
    val top: LibraryStackEntry? get() = entries.lastOrNull()

    fun push(destination: LibraryDestination) {
        entries.add(LibraryStackEntry(nextKey++, destination))
    }

    /**
     * Removes and returns the top entry, or null when the stack is already
     * empty. Empty is a designed-for input: a back event can be dispatched in
     * the same frame the stack emptied, before the BackHandler's enabled flag
     * catches up; the next press then gets the system default.
     */
    fun pop(): LibraryStackEntry? = entries.removeLastOrNull()
}

@Composable
internal fun LibraryDestinationScreen(
    session: OpenLibrary,
    destination: LibraryDestination,
    libraries: StateFlow<List<BridgeLibrary>>,
    onBack: () -> Unit,
    onPush: (LibraryDestination) -> Unit,
    actions: SessionActions,
) {
    val pushWork: (String) -> Unit = { onPush(LibraryDestination.Work(it)) }
    val pushAlbum: (String, String) -> Unit = { albumId, releaseId ->
        onPush(LibraryDestination.Album(AlbumTarget(albumId, releaseId)))
    }
    when (destination) {
        is LibraryDestination.Album -> {
            val target = destination.target
            AlbumDetailScreen(
                session = session,
                albumId = target.albumId,
                initialReleaseId = target.initialReleaseId,
                onBack = onBack,
            )
        }

        is LibraryDestination.Work -> {
            WorkDetailScreen(
                session = session,
                workId = destination.workId,
                onBack = onBack,
                onSelectWork = pushWork,
                onSelectAlbum = pushAlbum,
            )
        }

        is LibraryDestination.Composer -> {
            ComposerDetailScreen(
                session = session,
                artistId = destination.artistId,
                onBack = onBack,
                onSelectWork = pushWork,
                onSelectAlbum = pushAlbum,
            )
        }

        is LibraryDestination.Artist -> {
            ArtistDetailScreen(
                session = session,
                artistId = destination.artistId,
                onBack = onBack,
                onSelectAlbum = { onPush(LibraryDestination.Album(AlbumTarget(it))) },
            )
        }

        is LibraryDestination.Setting -> {
            SettingsDestinationScreen(session, destination, libraries, actions, onBack, onPush)
        }

        LibraryDestination.Downloads -> {
            DownloadsScreen(session = session, onBack = onBack)
        }
    }
}

/** Settings and the screens it opens. */
@Composable
private fun SettingsDestinationScreen(
    session: OpenLibrary,
    destination: LibraryDestination.Setting,
    libraries: StateFlow<List<BridgeLibrary>>,
    actions: SessionActions,
    onBack: () -> Unit,
    onPush: (LibraryDestination) -> Unit,
) {
    when (destination) {
        LibraryDestination.Settings -> {
            SettingsScreen(
                session = session,
                libraries = libraries,
                actions = actions,
                onBack = onBack,
                onPush = onPush,
            )
        }

        LibraryDestination.Members -> {
            MembersScreen(session = session, onBack = onBack)
        }

        LibraryDestination.SyncPending -> {
            SyncPendingScreen(session = session, actions = actions, onBack = onBack)
        }

        LibraryDestination.StorageSetup -> {
            StorageSetupScreen(session = session, oauthProviders = actions.oauthProviders, onBack = onBack)
        }
    }
}
