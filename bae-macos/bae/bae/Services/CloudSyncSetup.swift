#if BAE_OAUTH_PROVIDERS
    import BaeKit
    import Observation

    /// Full-edition cloud-home setup: provider sign-in, folder choice and
    /// connecting the chosen location. It owns the feature-gated bridge calls
    /// so the shared package never compiles against APIs absent from the
    /// baeium bridge. coven presents the sign-in and keeps the tokens.
    final class CloudSyncSetup: Sendable, Observable {
        let authenticate: @Sendable (BridgeOAuthAccess) async throws -> Void
        /// Connect a signed-in provider location, registering this device under
        /// its name.
        let connect:
            @Sendable (BridgeStorageLocation) async throws -> BridgeStorageSetup
        let oneDriveRoot: @Sendable () async throws -> BridgeStorageFolder
        let oneDriveFolders:
            @Sendable (BridgeStorageLocation) async throws ->
                [BridgeStorageFolder]
        let createOneDriveFolder:
            @Sendable (BridgeStorageLocation, String) async throws ->
                BridgeStorageFolder
        let dropboxSharedFolders:
            @Sendable () async throws -> [BridgeStorageFolder]
        #if BAE_CLOUDKIT
            /// Use the signed-in Apple account's iCloud, registering this device
            /// under its name.
            let useCloudKit: @Sendable () async throws -> BridgeStorageSetup
        #endif

        init(handle: any AppHandleProtocol, deviceName: String) {
            authenticate = { try await handle.authenticate(access: $0) }
            connect = {
                try await handle.setupOauthStorage(
                    location: $0,
                    deviceName: deviceName
                )
            }
            oneDriveRoot = { try await handle.onedriveRoot() }
            oneDriveFolders = { try await handle.onedriveFolders(parent: $0) }
            createOneDriveFolder = {
                try await handle.createOnedriveFolder(parent: $0, name: $1)
            }
            dropboxSharedFolders = { try await handle.dropboxSharedFolders() }
            #if BAE_CLOUDKIT
                useCloudKit = {
                    try await handle.useCloudkit(deviceName: deviceName)
                }
            #endif
        }

        #if DEBUG
            /// Previews: every operation throws.
            init() {
                authenticate = { _ in throw StubError.notImplemented }
                connect = { _ in throw StubError.notImplemented }
                oneDriveRoot = { throw StubError.notImplemented }
                oneDriveFolders = { _ in throw StubError.notImplemented }
                createOneDriveFolder = { _, _ in
                    throw StubError.notImplemented
                }
                dropboxSharedFolders = { throw StubError.notImplemented }
                #if BAE_CLOUDKIT
                    useCloudKit = { throw StubError.notImplemented }
                #endif
            }
        #endif
    }
#endif
