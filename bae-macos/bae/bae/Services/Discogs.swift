import BaeKit
import Foundation

/// Discogs API-key persistence — used by the Discogs key under the Import
/// settings' Search Discogs switch. `save` validates against Discogs before
/// storing and reports the outcome so the UI can keep a rejected draft;
/// `revalidate` re-checks a key saved offline. A key goes when the switch is
/// turned off, which core handles.
final class Discogs: Sendable, Observable {
    let saveDiscogsToken:
        @Sendable (_ token: String) async throws -> BridgeDiscogsSaveOutcome
    let revalidateDiscogsToken: @Sendable () async throws -> Void
    let getDiscogsToken: @Sendable () async throws -> String?

    init(
        saveDiscogsToken:
            @escaping @Sendable (String) async throws ->
            BridgeDiscogsSaveOutcome = { _ in
                throw StubError.notImplemented
            },
        revalidateDiscogsToken: @escaping @Sendable () async throws -> Void = {
        },
        getDiscogsToken: @escaping @Sendable () async throws -> String? = {
            nil
        }
    ) {
        self.saveDiscogsToken = saveDiscogsToken
        self.revalidateDiscogsToken = revalidateDiscogsToken
        self.getDiscogsToken = getDiscogsToken
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            saveDiscogsToken: { try await handle.saveDiscogsToken(token: $0) },
            revalidateDiscogsToken: {
                try await handle.revalidateDiscogsToken()
            },
            getDiscogsToken: { try await handle.getDiscogsToken() }
        )
    }

}
