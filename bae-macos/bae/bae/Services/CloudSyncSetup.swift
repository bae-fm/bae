#if BAE_OAUTH_PROVIDERS
    import BaeKit
    import Observation

    /// Full-edition storage setup: provider sign-in, folder choice and
    /// connecting the chosen location. It owns the feature-gated bridge calls
    /// so the shared package never compiles against APIs absent from the
    /// baeium bridge. coven presents the sign-in and keeps the tokens.
    final class CloudSyncSetup: Sendable, Observable {
        let signIn: @Sendable (BridgeCloudProvider) async throws -> Void
        /// Sign in again to the open library's provider, with the access its
        /// location needs, and reconnect that location under this device's
        /// name.
        let signInAgain:
            @Sendable (BridgeLibrarySetup) async throws -> BridgeStorageSetup
        /// Connect a signed-in provider location, registering this device under
        /// its name.
        let connect:
            @Sendable (BridgeStorageLocation, BridgeLibrarySetup) async throws
                ->
                BridgeStorageSetup
        let oneDriveRoot: @Sendable () async throws -> BridgeStorageFolder
        let oneDriveFolders:
            @Sendable (BridgeStorageLocation) async throws ->
                [BridgeStorageFolder]
        let createOneDriveFolder:
            @Sendable (BridgeStorageLocation, String) async throws ->
                BridgeStorageFolder
        let dropboxSharedFolders:
            @Sendable () async throws -> [BridgeStorageFolder]
        let createDropboxSharedFolder:
            @Sendable (String) async throws -> BridgeStorageFolder
        #if BAE_CLOUDKIT
            /// Use the signed-in Apple account's iCloud, registering this device
            /// under its name.
            let useCloudKit:
                @Sendable (BridgeLibrarySetup) async throws ->
                    BridgeStorageSetup
        #endif

        init(handle: any AppHandleProtocol, deviceName: String) {
            signIn = { try await handle.signIn(provider: $0) }
            signInAgain = {
                try await handle.signInAgain(deviceName: deviceName, setup: $0)
            }
            connect = {
                try await handle.setupOauthStorage(
                    location: $0,
                    deviceName: deviceName,
                    setup: $1
                )
            }
            oneDriveRoot = { try await handle.onedriveRoot() }
            oneDriveFolders = { try await handle.onedriveFolders(parent: $0) }
            createOneDriveFolder = {
                try await handle.createOnedriveFolder(parent: $0, name: $1)
            }
            dropboxSharedFolders = { try await handle.dropboxSharedFolders() }
            createDropboxSharedFolder = {
                try await handle.createDropboxSharedFolder(name: $0)
            }
            #if BAE_CLOUDKIT
                useCloudKit = {
                    try await handle.useCloudkit(
                        deviceName: deviceName,
                        setup: $0
                    )
                }
            #endif
        }

        /// The folder choice for a provider, signing in and listing through
        /// these calls.
        @MainActor
        func folderBrowser(
            _ provider: StorageFolderBrowser.Provider
        ) -> StorageFolderBrowser {
            StorageFolderBrowser(
                provider: provider,
                operations: StorageFolderBrowser.Operations(
                    signIn: signIn,
                    dropboxSharedFolders: dropboxSharedFolders,
                    createDropboxSharedFolder: createDropboxSharedFolder,
                    oneDriveRoot: oneDriveRoot,
                    oneDriveFolders: oneDriveFolders,
                    createOneDriveFolder: createOneDriveFolder
                )
            )
        }

        #if DEBUG
            /// Previews: every operation throws.
            init() {
                signIn = { _ in throw StubError.notImplemented }
                signInAgain = { _ in throw StubError.notImplemented }
                connect = { _, _ in throw StubError.notImplemented }
                oneDriveRoot = { throw StubError.notImplemented }
                oneDriveFolders = { _ in throw StubError.notImplemented }
                createOneDriveFolder = { _, _ in
                    throw StubError.notImplemented
                }
                dropboxSharedFolders = { throw StubError.notImplemented }
                createDropboxSharedFolder = { _ in
                    throw StubError.notImplemented
                }
                #if BAE_CLOUDKIT
                    useCloudKit = { _ in throw StubError.notImplemented }
                #endif
            }
        #endif
    }
#endif
