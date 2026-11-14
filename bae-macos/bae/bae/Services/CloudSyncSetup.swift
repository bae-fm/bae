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
            /// Use the signed-in Apple account's iCloud.
            let useCloudKit:
                @Sendable (BridgeLibrarySetup) async throws ->
                    BridgeStorageSetup
        #endif

        init(handle: any AppHandleProtocol) {
            signIn = { try await handle.signIn(provider: $0) }
            signInAgain = {
                try await handle.signInAgain(setup: $0)
            }
            connect = {
                try await handle.setupOauthStorage(
                    location: $0,
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
                    try await handle.useCloudkit(setup: $0)
                }
            #endif
        }

        /// The folder choice for a provider, signing in and listing through
        /// these calls.
        @MainActor
        func folderBrowser(
            _ provider: StorageFolderBrowser.Provider
        ) -> StorageFolderBrowser {
            let dropboxSharedFolders = dropboxSharedFolders
            let createDropboxSharedFolder = createDropboxSharedFolder
            let oneDriveRoot = oneDriveRoot
            let oneDriveFolders = oneDriveFolders
            let createOneDriveFolder = createOneDriveFolder
            let operations: StorageFolderBrowser.Operations =
                switch provider {
                case .dropbox:
                    StorageFolderBrowser.Operations(
                        signIn: signIn,
                        top: {
                            StorageFolderBrowser.Listing(
                                path: [],
                                folders: try await dropboxSharedFolders()
                            )
                        },
                        inside: nil,
                        make: { _, name in
                            try await createDropboxSharedFolder(name)
                        }
                    )
                case .oneDrive:
                    StorageFolderBrowser.Operations(
                        signIn: signIn,
                        top: {
                            let root = try await oneDriveRoot()
                            return StorageFolderBrowser.Listing(
                                path: [root],
                                folders: try await oneDriveFolders(
                                    root.location
                                )
                            )
                        },
                        inside: { try await oneDriveFolders($0.location) },
                        make: { parent, name in
                            guard let parent else {
                                preconditionFailure(
                                    "OneDrive folders are made inside one"
                                )
                            }
                            return try await createOneDriveFolder(
                                parent.location,
                                name
                            )
                        }
                    )
                }
            return StorageFolderBrowser(
                provider: provider,
                operations: operations
            )
        }

        #if DEBUG
            /// Previews: every operation throws.
            convenience init() {
                self.init(signIn: { _ in throw StubError.notImplemented })
            }

            /// Previews and shots: every operation throws but the sign-in,
            /// which is `signIn`.
            init(
                signIn:
                    @escaping @Sendable (BridgeCloudProvider) async throws
                    -> Void
            ) {
                self.signIn = signIn
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
