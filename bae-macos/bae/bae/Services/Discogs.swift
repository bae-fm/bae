import BaeKit
import Foundation

/// Discogs API-key persistence — used by the Discogs key under the Import
/// settings' Sources section. `save` validates against Discogs before storing
/// and reports the outcome so the UI can keep a rejected draft; `revalidate`
/// re-checks a key saved offline.
final class Discogs: Sendable, Observable {
    let saveDiscogsToken:
        @Sendable (_ token: String) async throws -> BridgeDiscogsSaveOutcome
    let revalidateDiscogsToken: @Sendable () async throws -> Void
    let removeDiscogsToken: @Sendable () async throws -> Void
    let getDiscogsToken: @Sendable () async throws -> String?

    init(
        saveDiscogsToken:
            @escaping @Sendable (String) async throws ->
            BridgeDiscogsSaveOutcome = { _ in
                throw StubError.notImplemented
            },
        revalidateDiscogsToken: @escaping @Sendable () async throws -> Void = {
        },
        removeDiscogsToken: @escaping @Sendable () async throws -> Void = {},
        getDiscogsToken: @escaping @Sendable () async throws -> String? = {
            nil
        }
    ) {
        self.saveDiscogsToken = saveDiscogsToken
        self.revalidateDiscogsToken = revalidateDiscogsToken
        self.removeDiscogsToken = removeDiscogsToken
        self.getDiscogsToken = getDiscogsToken
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            saveDiscogsToken: { try await handle.saveDiscogsToken(token: $0) },
            revalidateDiscogsToken: {
                try await handle.revalidateDiscogsToken()
            },
            removeDiscogsToken: { try await handle.removeDiscogsToken() },
            getDiscogsToken: { try await handle.getDiscogsToken() }
        )
    }

    #if DEBUG
        static func stub() -> Discogs { Discogs() }
    #endif
}
