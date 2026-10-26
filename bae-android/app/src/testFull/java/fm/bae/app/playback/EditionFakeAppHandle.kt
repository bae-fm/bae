package fm.bae.app.playback

import fm.bae.app.OpenLibraryHandle
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeStorageFolder
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup

/** The calls only the full edition's bridge has (its cloud provider sign-in), which no test fakes. */
internal abstract class EditionFakeAppHandle : OpenLibraryHandle {
    override suspend fun createOnedriveFolder(
        parent: BridgeStorageLocation,
        name: String,
    ): BridgeStorageFolder = unfaked("createOnedriveFolder")

    override suspend fun dropboxSharedFolders(): List<BridgeStorageFolder> = unfaked("dropboxSharedFolders")

    override suspend fun onedriveFolders(parent: BridgeStorageLocation): List<BridgeStorageFolder> = unfaked("onedriveFolders")

    override suspend fun onedriveRoot(): BridgeStorageFolder = unfaked("onedriveRoot")

    override suspend fun setupOauthStorage(
        location: BridgeStorageLocation,
        deviceName: String,
    ): BridgeStorageSetup = unfaked("setupOauthStorage")

    override suspend fun signIn(provider: BridgeCloudProvider) {
        unfaked("signIn")
    }

    override suspend fun signInAgain(deviceName: String): BridgeStorageSetup = unfaked("signInAgain")
}
