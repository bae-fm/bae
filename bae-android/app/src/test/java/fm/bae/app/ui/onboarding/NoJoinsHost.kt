package fm.bae.app.ui.onboarding

import fm.bae.app.playback.unfaked
import uniffi.bae_bridge.BootstrapCancellation
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeJoinControl
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeLibraryOpening
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeReplacedBy
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeSavedRestoreCode
import uniffi.bae_bridge.BridgeSavedRestoreEntry
import uniffi.bae_bridge.BridgeUnfinishedJoin
import uniffi.bae_bridge.BridgeUnfinishedRemoval
import uniffi.bae_bridge.BridgeUnsentWork
import uniffi.bae_bridge.OpenedLibrary

/**
 * A device with no join under way and no restore codes saved on it; opening
 * the code field and going back call no other core.
 */
internal open class NoJoinsHost : EditionFakeBridgeHost() {
    override suspend fun unfinishedJoins(): List<BridgeUnfinishedJoin> = emptyList()

    override suspend fun savedRestoreCodes(): List<BridgeSavedRestoreEntry> = emptyList()

    override suspend fun createLibrary(
        name: String?,
        setup: BridgeLibrarySetup,
    ): OpenedLibrary = unfaked("createLibrary")

    override suspend fun finishRemovals(setup: BridgeLibrarySetup) {
        unfaked("finishRemovals")
    }

    override suspend fun flushDiagnostics() {
        unfaked("flushDiagnostics")
    }

    override suspend fun listLibraries(): List<BridgeLibrary> = unfaked("listLibraries")

    override suspend fun removeUnopenableLibrary(
        libraryId: String,
        closing: BridgeLibraryClosing?,
    ): BridgeLibraryRemoval = unfaked("removeUnopenableLibrary")

    override suspend fun finishDeletion(
        libraryId: String,
        closing: BridgeLibraryClosing?,
    ): BridgeLibraryRemoval = unfaked("finishDeletion")

    override suspend fun unfinishedRemoval(): BridgeUnfinishedRemoval? = unfaked("unfinishedRemoval")

    override suspend fun unsentBeforeRemoval(
        libraryId: String,
        opening: BridgeLibraryOpening,
    ): BridgeUnsentWork = unfaked("unsentBeforeRemoval")

    override suspend fun forgetSavedRestoreCode(libraryId: String) {
        unfaked("forgetSavedRestoreCode")
    }

    override suspend fun joinWithInvite(
        code: String,
        deviceName: String,
        setup: BridgeLibrarySetup,
        control: BridgeJoinControl,
    ): BridgeBootstrapResult = unfaked("joinWithInvite")

    override fun prepareDeviceReset(
        libraryId: String,
        deviceName: String,
    ): BridgeDeviceReset = unfaked("prepareDeviceReset")

    override suspend fun restoreFromCode(
        code: String,
        credentials: BridgeS3Credentials?,
        deviceName: String,
        setup: BridgeLibrarySetup,
        cancellation: BootstrapCancellation,
    ): BridgeBootstrapResult = unfaked("restoreFromCode")

    override suspend fun restoreFromSavedCode(
        saved: BridgeSavedRestoreCode,
        credentials: BridgeS3Credentials?,
        deviceName: String,
        setup: BridgeLibrarySetup,
        cancellation: BootstrapCancellation,
    ): BridgeBootstrapResult = unfaked("restoreFromSavedCode")

    override suspend fun restoreReplacingDevice(
        libraryId: String,
        code: String,
        credentials: BridgeS3Credentials?,
        deviceName: String,
        setup: BridgeLibrarySetup,
        opening: BridgeLibraryOpening,
        cancellation: BootstrapCancellation,
    ): BridgeBootstrapResult = unfaked("restoreReplacingDevice")

    override suspend fun resumeReplacement(
        libraryId: String,
        by: BridgeReplacedBy,
        setup: BridgeLibrarySetup,
        cancellation: BootstrapCancellation,
    ): BridgeBootstrapResult = unfaked("resumeReplacement")

    override suspend fun resumeJoin(
        join: BridgeUnfinishedJoin,
        setup: BridgeLibrarySetup,
        control: BridgeJoinControl,
    ): BridgeBootstrapResult = unfaked("resumeJoin")
}
