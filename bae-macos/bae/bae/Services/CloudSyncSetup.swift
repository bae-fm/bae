#if BAE_OAUTH_PROVIDERS

    import BaeKit
    import Observation

    /// Full-edition cloud-provider setup. It owns the feature-gated bridge
    /// capabilities so the shared package never compiles against APIs absent
    /// from the baeium bridge.
    final class CloudSyncSetup: @unchecked Sendable, Observable {
        private let connectOAuthOperation:
            @Sendable (BridgeCloudProvider, BridgeHomeStorage) async throws
                -> Void
        private let connectCloudKitOperation:
            @Sendable (BridgeHomeStorage) async throws -> Void

        init(handle: any AppHandleProtocol) {
            connectOAuthOperation = { provider, storage in
                try await handle.signInCloudProvider(
                    provider: provider,
                    storage: storage
                )
            }
            connectCloudKitOperation = { storage in
                try await CloudKitService.bae().checkAccountAvailable()
                try await handle.useCloudkit(storage: storage)
            }
        }

        #if DEBUG
            init(
                connectOAuth:
                    @escaping @Sendable (
                        BridgeCloudProvider, BridgeHomeStorage
                    ) async throws -> Void,
                connectCloudKit:
                    @escaping @Sendable (BridgeHomeStorage) async throws -> Void
            ) {
                connectOAuthOperation = connectOAuth
                connectCloudKitOperation = connectCloudKit
            }
        #endif

        func connectOAuth(
            provider: BridgeCloudProvider,
            storage: BridgeHomeStorage
        ) async throws {
            try await connectOAuthOperation(provider, storage)
        }

        func connectCloudKit(storage: BridgeHomeStorage) async throws {
            try await connectCloudKitOperation(storage)
        }
    }

#endif
