package fm.bae.app.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateListOf
import fm.bae.app.OpenLibrary
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

    data object Members : LibraryDestination

    data object Settings : LibraryDestination
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
    onSwitchLibrary: (BridgeLibrary) -> Unit,
    onLeaveLibrary: () -> Unit,
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

        LibraryDestination.Members -> {
            MembersScreen(session = session, onBack = onBack)
        }

        LibraryDestination.Settings -> {
            SettingsScreen(
                session = session,
                libraries = libraries,
                onBack = onBack,
                onManageDevices = { onPush(LibraryDestination.Members) },
                onSwitchLibrary = onSwitchLibrary,
                onLeaveLibrary = onLeaveLibrary,
            )
        }
    }
}
