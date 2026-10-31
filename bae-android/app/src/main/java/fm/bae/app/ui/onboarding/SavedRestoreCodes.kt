package fm.bae.app.ui.onboarding

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHostInterface
import uniffi.bae_bridge.BridgeSavedRestoreCode
import uniffi.bae_bridge.BridgeSavedRestoreCodeInfo
import uniffi.bae_bridge.BridgeSavedRestoreEntry
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeUnreadableSavedCode

private val logger = BaeLogger("bae.SavedRestoreCodes")

/**
 * A restore code coven saved on this phone that bae can restore from, with
 * what it says read off it once: the library's name as one of the person's
 * devices last saw it, and where the library is stored.
 */
internal class SavedRestoreCode(
    val code: BridgeSavedRestoreCode,
    val info: BridgeSavedRestoreCodeInfo,
) {
    /** Whether restoring needs the S3 access key typed in: an S3 library with no key saved beside the code. */
    val needsS3Key: Boolean
        get() = info.location is BridgeStorageLocation.S3 && !info.hasS3Credentials
}

/**
 * One library's entry in this phone's Block Store, as the welcome lists it: a
 * code to restore from, or one bae can't read, which the person can only
 * delete. Either is deleted by its library id.
 */
internal sealed interface SavedRestoreEntry {
    val libraryId: String

    data class Readable(
        val saved: SavedRestoreCode,
    ) : SavedRestoreEntry {
        override val libraryId: String
            get() = saved.info.libraryId
    }

    data class Unreadable(
        override val libraryId: String,
        val reason: BridgeUnreadableSavedCode,
    ) : SavedRestoreEntry

    companion object {
        fun of(entry: BridgeSavedRestoreEntry): SavedRestoreEntry =
            when (entry) {
                is BridgeSavedRestoreEntry.Readable -> Readable(SavedRestoreCode(entry.code, entry.code.info()))
                is BridgeSavedRestoreEntry.Unreadable -> Unreadable(entry.libraryId, entry.reason)
            }
    }
}

/** The saved codes as last read, or why they couldn't be read. */
internal sealed interface SavedCodesList {
    data object Loading : SavedCodesList

    data class Loaded(
        val entries: List<SavedRestoreEntry>,
    ) : SavedCodesList

    data class Failed(
        val error: DisplayError,
    ) : SavedCodesList
}

/**
 * The restore codes coven saved in this phone's Block Store for libraries not
 * on it: the codes this phone saved, and those a new phone received from the
 * phone it was set up from. Deleting one removes it, and the S3 key saved
 * beside it, from this phone only: other phones keep their own, and the
 * library, its storage and every device's copy stay. coven holds the codes;
 * this reads them each time the welcome shows them, and again after a delete.
 */
internal class SavedRestoreCodes(
    private val scope: CoroutineScope,
    private val host: BridgeHostInterface,
    private val errorDisplay: (BridgeException) -> DisplayError?,
) {
    var list by mutableStateOf<SavedCodesList>(SavedCodesList.Loading)
        private set

    /** The entry the person is asked to confirm deleting. */
    var pendingDelete by mutableStateOf<SavedRestoreEntry?>(null)
        private set

    /** The library whose code is being deleted. */
    var deletingLibraryId by mutableStateOf<String?>(null)
        private set

    /** Why the last delete failed. */
    var deleteError by mutableStateOf<DisplayError?>(null)
        private set

    /** Read the saved codes from coven. */
    suspend fun load() {
        val read =
            try {
                SavedCodesList.Loaded(host.savedRestoreCodes().map(SavedRestoreEntry::of))
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                // A refused read isn't the same as no saved codes, so it shows
                // as a failure. Nothing to show means core reported a cancellation.
                val shown = errorDisplay(e)
                if (shown == null) {
                    logger.debug("listing the saved restore codes was cancelled", e)
                    return
                }
                logger.error("listing the saved restore codes failed", e)
                SavedCodesList.Failed(shown)
            }
        list = read
    }

    /** Ask the person to confirm deleting [entry]'s code. */
    fun askToDelete(entry: SavedRestoreEntry) {
        pendingDelete = entry
    }

    /** Close the delete confirmation without deleting. */
    fun dismissDelete() {
        pendingDelete = null
    }

    /**
     * Delete [entry]'s code from this phone once the person confirmed, then
     * read the codes again, which no longer list it. A failure leaves the code
     * listed and shows as [deleteError]; deleting again finishes a delete that
     * stopped partway.
     */
    fun delete(entry: SavedRestoreEntry) {
        pendingDelete = null
        deleteError = null
        deletingLibraryId = entry.libraryId
        scope.launch {
            try {
                host.forgetSavedRestoreCode(entry.libraryId)
                load()
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                val shown = errorDisplay(e)
                if (shown == null) {
                    logger.debug("deleting the saved restore code was cancelled", e)
                } else {
                    logger.error("deleting the saved restore code failed", e)
                    deleteError = shown
                }
            } finally {
                deletingLibraryId = null
            }
        }
    }
}
