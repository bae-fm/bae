package fm.bae.app.ui.settings

import androidx.annotation.StringRes
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OAuthProviders
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeStorageFailure
import uniffi.bae_bridge.BridgeStorageFolder

private val logger = BaeLogger("bae.FolderBrowser")

/** The folders shown, and the path of folders opened to reach them: empty for a flat list. */
internal data class FolderListing(
    val path: List<BridgeStorageFolder>,
    val folders: List<BridgeStorageFolder>,
)

/**
 * A provider's folder calls, which the folder choice runs on. [top] lists the first folders, [inside]
 * the folders inside one (null when the provider's folders are one flat list), and [make] makes a
 * folder named so in the folder shown, or at the top of a flat list, and returns it.
 */
internal class FolderCalls(
    val top: suspend () -> FolderListing,
    val inside: (suspend (BridgeStorageFolder) -> List<BridgeStorageFolder>)?,
    val make: suspend (parent: BridgeStorageFolder?, name: String) -> BridgeStorageFolder,
) {
    companion object {
        /** The Dropbox folders shared with the signed-in account, one flat list. */
        fun dropbox(
            handle: AppHandleInterface,
            providers: OAuthProviders,
        ) = FolderCalls(
            top = { FolderListing(emptyList(), providers.dropboxSharedFolders(handle)) },
            inside = null,
            make = { _, name -> providers.createDropboxSharedFolder(handle, name) },
        )

        /** The signed-in OneDrive, browsed from its top. */
        fun oneDrive(
            handle: AppHandleInterface,
            providers: OAuthProviders,
        ) = FolderCalls(
            top = {
                val root = providers.oneDriveRoot(handle)
                FolderListing(listOf(root), providers.oneDriveFolders(handle, root.location))
            },
            inside = { providers.oneDriveFolders(handle, it.location) },
            make = { parent, name ->
                val inside = checkNotNull(parent) { "OneDrive folders are made inside one" }
                providers.createOneDriveFolder(handle, inside.location, name)
            },
        )
    }
}

/** What a provider's folder choice says, by string resource, and the name its operations log under. */
internal class FolderWords(
    @StringRes val heading: Int,
    @StringRes val empty: Int,
    @StringRes val note: Int?,
    @StringRes val reading: Int,
    @StringRes val nameTaken: Int,
    val provider: String,
)

internal val DROPBOX_WORDS =
    FolderWords(
        heading = R.string.storage_dropbox_heading,
        empty = R.string.storage_dropbox_empty,
        note = R.string.storage_dropbox_note,
        reading = R.string.storage_dropbox_reading,
        nameTaken = R.string.storage_dropbox_name_taken,
        provider = "Dropbox",
    )

internal val ONEDRIVE_WORDS =
    FolderWords(
        heading = R.string.storage_onedrive_heading,
        empty = R.string.storage_onedrive_empty,
        note = null,
        reading = R.string.storage_onedrive_reading,
        nameTaken = R.string.storage_onedrive_name_taken,
        provider = "OneDrive",
    )

/** A move the picker couldn't make: why, when core has something to show, and the same move again. */
internal class FolderFailure(
    val error: DisplayError?,
    val retry: suspend () -> Unit,
)

/**
 * Where choosing a folder stands, for Dropbox and OneDrive alike. It moves into a folder only once that
 * folder's listing has loaded, so the path, the folders and Set up always belong to one listing. A
 * move that fails keeps the listing shown, with the failure and Retry; a failed first listing shows
 * only those.
 */
internal class FolderBrowser(
    private val calls: FolderCalls,
    private val errors: ErrorDisplays,
    private val provider: String,
) {
    /** The listing shown, or null until the first one has loaded. */
    var shown by mutableStateOf<FolderListing?>(null)
        private set

    /** The last move's failure, until the next move starts. */
    var failure by mutableStateOf<FolderFailure?>(null)
        private set

    /** The folder chosen from the list; Set up uses it. */
    var selected by mutableStateOf<BridgeStorageFolder?>(null)
        private set

    /** The name the provider refused because a folder of that name is already there. */
    var takenName by mutableStateOf<String?>(null)
        private set

    /** The folder setup found holding other files, which the person replaces by choosing another or making one. */
    var notEmpty by mutableStateOf<BridgeStorageFolder?>(null)
        private set

    /** What is in flight, a move, a new folder or the setup; while one is, nothing else can start. */
    var work by mutableStateOf<FolderWork?>(null)

    /** Whether anything is in flight. */
    val busy: Boolean get() = work != null

    /** Whether a folder opens into the folders inside it, so the list shows where the person is. */
    val nests: Boolean get() = calls.inside != null

    /** The folder Set up uses: the one chosen, unless setup just found it holding other files. */
    val target: BridgeStorageFolder? get() = selected?.takeIf { it != notEmpty }

    fun select(folder: BridgeStorageFolder) {
        selected = folder
    }

    companion object {
        /** A browser already showing [shown], for a screenshot scene; its calls all fail. */
        fun staged(
            shown: FolderListing,
            selected: BridgeStorageFolder? = null,
            notEmpty: BridgeStorageFolder? = null,
            nests: Boolean = false,
            errors: ErrorDisplays,
        ): FolderBrowser {
            val unstaged: suspend () -> Nothing = { error("a staged folder browser makes no calls") }
            val calls =
                FolderCalls(
                    top = { unstaged() },
                    inside = if (nests) ({ unstaged() }) else null,
                    make = { _, _ -> unstaged() },
                )
            return FolderBrowser(calls, errors, "staged").also {
                it.shown = shown
                it.selected = selected
                it.notEmpty = notEmpty
            }
        }
    }

    suspend fun openTop() {
        move("list the $provider folders", ::openTop) { calls.top() }
    }

    /** Go into [folder], one of [from]'s. */
    suspend fun open(
        from: FolderListing,
        folder: BridgeStorageFolder,
    ) {
        val inside = checkNotNull(calls.inside) { "only a nesting provider opens a folder" }
        move("list the $provider folders", { open(from, folder) }) {
            FolderListing(from.path + folder, inside(folder))
        }
    }

    /** Go back to the folder [from] was opened from. */
    suspend fun up(from: FolderListing) {
        val inside = checkNotNull(calls.inside) { "only a nesting provider has a parent" }
        val path = from.path.dropLast(1)
        move("list the $provider folders", { up(from) }) {
            FolderListing(path, inside(path.last()))
        }
    }

    /**
     * Make a folder named [name] in [from]'s folder, add it to the list and choose it. A name already
     * taken is not a failure: the list stays as it was and [takenName] names it.
     */
    suspend fun create(
        from: FolderListing,
        name: String,
    ) {
        takenName = null
        step(FolderWork.CREATING, "create a $provider folder", { create(from, name) }) {
            try {
                val created = calls.make(from.path.lastOrNull(), name)
                shown = from.copy(folders = from.folders + created)
                selected = created
            } catch (e: BridgeException.Diagnostic) {
                val category = e.category
                if (category is BridgeErrorCategory.Storage && category.failure is BridgeStorageFailure.AlreadyExists) {
                    takenName = name
                } else {
                    throw e
                }
            }
        }
    }

    /**
     * Offered a failed setup before the screen shows it: a folder holding other files is named in the
     * list, which the person chooses again from. True when it was.
     */
    fun place(error: BridgeException): Boolean {
        val failure = ((error as? BridgeException.Diagnostic)?.category as? BridgeErrorCategory.Storage)?.failure
        val chosen = selected
        if (failure !is BridgeStorageFailure.OwnLocationRequired || chosen == null) return false
        notEmpty = chosen
        return true
    }

    private suspend fun move(
        operation: String,
        retry: suspend () -> Unit,
        load: suspend () -> FolderListing,
    ) {
        step(FolderWork.LISTING, operation, retry) {
            shown = load()
            selected = null
        }
    }

    private suspend fun step(
        work: FolderWork,
        operation: String,
        retry: suspend () -> Unit,
        action: suspend () -> Unit,
    ) {
        this.work = work
        failure = null
        notEmpty = null
        performBridgeAction(logger, operation, errors, { error -> failure = FolderFailure(error, retry) }) {
            action()
        }
        this.work = null
    }
}

/** What a folder choice has in flight. */
internal enum class FolderWork {
    /** Listing a folder's folders. */
    LISTING,

    /** Making a new folder. */
    CREATING,

    /** Setting the folder chosen up as the library's storage. */
    SETTING_UP,
}
