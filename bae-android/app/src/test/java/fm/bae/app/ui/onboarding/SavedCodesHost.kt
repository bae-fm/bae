package fm.bae.app.ui.onboarding

import kotlinx.coroutines.awaitCancellation
import uniffi.bae_bridge.BootstrapCallback
import uniffi.bae_bridge.BootstrapCancellation
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeDropboxAccess
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeOAuthAccess
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeSavedRestoreCode
import uniffi.bae_bridge.BridgeSavedRestoreCodeInfo
import uniffi.bae_bridge.BridgeSavedRestoreEntry
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeUnreadableSavedCode
import uniffi.bae_bridge.NoHandle

/** A saved code with no Rust object behind it: it answers [info] itself. */
internal class FakeSavedRestoreCode(
    private val fixture: BridgeSavedRestoreCodeInfo,
) : BridgeSavedRestoreCode(NoHandle) {
    override fun info(): BridgeSavedRestoreCodeInfo = fixture
}

/** A Dropbox library's saved code, which restores after the person signs in. */
internal val dropboxCode =
    BridgeSavedRestoreCodeInfo(
        name = "Home Library",
        libraryId = "saved-dropbox",
        location = BridgeStorageLocation.Dropbox(BridgeDropboxLocation.AppFolder, accountLabel = "ana@example.com"),
        oauthAccess = BridgeOAuthAccess.Dropbox(BridgeDropboxAccess.APP_FOLDER),
        hasS3Credentials = false,
    )

/** An S3 library's saved code, with its access key saved beside it or not. */
internal fun s3Code(hasS3Credentials: Boolean) =
    BridgeSavedRestoreCodeInfo(
        name = "Home Library",
        libraryId = "saved-s3",
        location =
            BridgeStorageLocation.S3(
                bucket = "music-library",
                region = "us-east-1",
                endpoint = "storage.example.com",
                profile = null,
                prefix = "",
            ),
        oauthAccess = null,
        hasS3Credentials = hasS3Credentials,
    )

/** An entry saved in a format bae can't read. */
internal const val UNREADABLE_LIBRARY = "6f1c2a9e-4b7d-4e2a-9c3f-2d8e5b1a7c40"

/**
 * This phone's Block Store as onboarding reads, deletes and restores from it:
 * [readable] codes, then [unreadable] entries. A restore from a saved code is
 * recorded and runs until the test ends, unless [savedCodeGone] says the code
 * was deleted since it was listed.
 */
internal class SavedCodesHost(
    readable: List<BridgeSavedRestoreCodeInfo>,
    unreadable: List<String> = emptyList(),
) : NoJoinsHost() {
    private val readable = readable.toMutableList()
    private val unreadable = unreadable.toMutableList()
    val forgotten = mutableListOf<String>()
    val restored = mutableListOf<Pair<String, BridgeS3Credentials?>>()

    /** Refuse reads, as a Block Store Google Play services can't reach would. */
    var refuseReads = false

    /** Refuse deletes, leaving every entry in place. */
    var refuseDeletes = false

    /** Answer a restore from a saved code as coven does once the code is gone. */
    var savedCodeGone = false

    override suspend fun savedRestoreCodes(): List<BridgeSavedRestoreEntry> {
        if (refuseReads) throw BridgeException.Diagnostic(BridgeErrorCategory.Keyring, "Block Store refused the read")
        return readable.map { BridgeSavedRestoreEntry.Readable(FakeSavedRestoreCode(it)) } +
            unreadable.map { BridgeSavedRestoreEntry.Unreadable(it, BridgeUnreadableSavedCode.FORMAT) }
    }

    override suspend fun forgetSavedRestoreCode(libraryId: String) {
        if (refuseDeletes) throw BridgeException.Diagnostic(BridgeErrorCategory.Keyring, "Block Store refused the delete")
        forgotten += libraryId
        readable.removeAll { it.libraryId == libraryId }
        unreadable.remove(libraryId)
    }

    override suspend fun restoreFromSavedCode(
        saved: BridgeSavedRestoreCode,
        credentials: BridgeS3Credentials?,
        deviceName: String,
        callback: BootstrapCallback,
        cancellation: BootstrapCancellation,
    ): BridgeBootstrapResult {
        restored += saved.info().libraryId to credentials
        if (savedCodeGone) return BridgeBootstrapResult.SavedCodeRemoved
        awaitCancellation()
    }
}
