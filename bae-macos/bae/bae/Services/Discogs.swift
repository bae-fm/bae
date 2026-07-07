import BaeKit
import Foundation

/// Discogs API-key persistence — used by the Discogs settings tab. `save`
/// validates against Discogs before storing and reports the outcome so the UI
/// can keep a rejected draft; `revalidate` re-checks a key saved offline.
final class Discogs: Sendable, Observable {
    let saveDiscogsToken:
        @Sendable (_ token: String) async throws -> BridgeDiscogsSaveOutcome
    let revalidateDiscogsToken: @Sendable () async throws -> Void
    let removeDiscogsToken: @Sendable () throws -> Void
    let getDiscogsToken: @Sendable () throws -> String?

    init(
        saveDiscogsToken:
            @escaping @Sendable (String) async throws ->
            BridgeDiscogsSaveOutcome = {
                _ in .valid
            },
        revalidateDiscogsToken: @escaping @Sendable () async throws -> Void = {
        },
        removeDiscogsToken: @escaping @Sendable () throws -> Void = {},
        getDiscogsToken: @escaping @Sendable () throws -> String? = { nil }
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
            removeDiscogsToken: { try handle.removeDiscogsToken() },
            getDiscogsToken: { try handle.getDiscogsToken() }
        )
    }

    // periphery:ignore
    static let stub = Discogs()
}
