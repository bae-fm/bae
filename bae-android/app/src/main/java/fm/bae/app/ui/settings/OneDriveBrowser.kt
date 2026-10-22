package fm.bae.app.ui.settings

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OAuthProviders
import fm.bae.app.performBridgeAction
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeStorageFolder

private val logger = BaeLogger("bae.OneDriveBrowser")

/**
 * A folder open in the OneDrive picker: the folders opened from the top to
 * reach it, it last, and the folders it holds.
 */
internal data class OneDriveListing(
    val path: List<BridgeStorageFolder>,
    val children: List<BridgeStorageFolder>,
) {
    val folder: BridgeStorageFolder get() = path.last()
}

/** A move the picker couldn't make: why, when core has something to show, and the same move again. */
internal class OneDriveFailure(
    val error: DisplayError?,
    val retry: suspend () -> Unit,
)

/**
 * Where browsing the signed-in OneDrive stands. It moves into a folder only
 * once that folder's listing has loaded, so the heading, the folders and "Use
 * this folder" always belong to one folder. A move that fails keeps the folder
 * shown, with the failure and Retry; a failed first listing shows only those.
 */
internal class OneDriveBrowser(
    private val handle: AppHandle,
    private val providers: OAuthProviders,
    private val errors: ErrorDisplays,
) {
    /** The folder shown, or null until the top of the drive has loaded. */
    var shown by mutableStateOf<OneDriveListing?>(null)
        private set

    /** The last move's failure, until the next move starts. */
    var failure by mutableStateOf<OneDriveFailure?>(null)
        private set

    /** A move or the setup is in flight; nothing else can start. */
    var busy by mutableStateOf(false)

    suspend fun openRoot() {
        move("open the OneDrive root", ::openRoot) {
            val root = providers.oneDriveRoot(handle)
            OneDriveListing(listOf(root), providers.oneDriveFolders(handle, root.location))
        }
    }

    /** Go into [folder], one of [from]'s. */
    suspend fun open(
        from: OneDriveListing,
        folder: BridgeStorageFolder,
    ) {
        move("list OneDrive folders", { open(from, folder) }) {
            OneDriveListing(from.path + folder, providers.oneDriveFolders(handle, folder.location))
        }
    }

    /** Go back to the folder [from] was opened from. */
    suspend fun up(from: OneDriveListing) {
        val path = from.path.dropLast(1)
        move("list OneDrive folders", { up(from) }) {
            OneDriveListing(path, providers.oneDriveFolders(handle, path.last().location))
        }
    }

    /**
     * Make a folder named [name] inside [from]'s folder and go into it. Once it
     * exists, a failed listing's Retry lists it again rather than making a
     * second one.
     */
    suspend fun create(
        from: OneDriveListing,
        name: String,
    ) {
        var created: BridgeStorageFolder? = null
        step("create a OneDrive folder", { create(from, name) }) {
            created = providers.createOneDriveFolder(handle, from.folder.location, name)
        }
        created?.let { open(from, it) }
    }

    private suspend fun move(
        operation: String,
        retry: suspend () -> Unit,
        load: suspend () -> OneDriveListing,
    ) {
        step(operation, retry) { shown = load() }
    }

    private suspend fun step(
        operation: String,
        retry: suspend () -> Unit,
        action: suspend () -> Unit,
    ) {
        busy = true
        failure = null
        performBridgeAction(logger, operation, errors, { error -> failure = OneDriveFailure(error, retry) }) {
            action()
        }
        busy = false
    }
}
