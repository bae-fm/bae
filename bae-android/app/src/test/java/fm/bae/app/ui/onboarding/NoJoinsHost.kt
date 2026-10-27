package fm.bae.app.ui.onboarding

import fm.bae.app.playback.unfaked
import uniffi.bae_bridge.BootstrapCallback
import uniffi.bae_bridge.BootstrapCancellation
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeJoinControl
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeSavedRestoreCode
import uniffi.bae_bridge.BridgeSavedRestoreEntry
import uniffi.bae_bridge.BridgeUnfinishedJoin
import uniffi.bae_bridge.BridgeUnfinishedRemoval
import uniffi.bae_bridge.BridgeUnsentWork
import uniffi.bae_bridge.OpenedLibrary

/** A device with no join under way; opening the code field and going back call no other core. */
internal open class NoJoinsHost : EditionFakeBridgeHost() {
    override suspend fun unfinishedJoins(): List<BridgeUnfinishedJoin> = emptyList()

    override suspend fun createLibrary(name: String?): OpenedLibrary = unfaked("createLibrary")

    override suspend fun finishRemovals() {
        unfaked("finishRemovals")
    }

    override suspend fun flushDiagnostics() {
        unfaked("flushDiagnostics")
    }

    override suspend fun listLibraries(): List<BridgeLibrary> = unfaked("listLibraries")

    override suspend fun removeUnopenableLibrary(libraryId: String): BridgeLibraryRemoval = unfaked("removeUnopenableLibrary")

    override suspend fun unfinishedRemoval(): BridgeUnfinishedRemoval? = unfaked("unfinishedRemoval")

    override suspend fun unsentBeforeRemoval(libraryId: String): BridgeUnsentWork = unfaked("unsentBeforeRemoval")

    override suspend fun forgetSavedRestoreCode(libraryId: String) {
        unfaked("forgetSavedRestoreCode")
    }

    override suspend fun joinWithInvite(
        code: String,
        deviceName: String,
        callback: BootstrapCallback,
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
        callback: BootstrapCallback,
        cancellation: BootstrapCancellation,
    ): BridgeBootstrapResult = unfaked("restoreFromCode")

    override suspend fun restoreFromKeychain(
        saved: BridgeSavedRestoreCode,
        credentials: BridgeS3Credentials?,
        deviceName: String,
        callback: BootstrapCallback,
        cancellation: BootstrapCancellation,
    ): BridgeBootstrapResult = unfaked("restoreFromKeychain")

    override suspend fun restoreReplacingDevice(
        libraryId: String,
        code: String,
        credentials: BridgeS3Credentials?,
        deviceName: String,
        callback: BootstrapCallback,
        cancellation: BootstrapCancellation,
    ): BridgeBootstrapResult = unfaked("restoreReplacingDevice")

    override suspend fun resumeJoin(
        join: BridgeUnfinishedJoin,
        callback: BootstrapCallback,
        control: BridgeJoinControl,
    ): BridgeBootstrapResult = unfaked("resumeJoin")

    override suspend fun savedRestoreCodes(): List<BridgeSavedRestoreEntry> = unfaked("savedRestoreCodes")
}
