import BaeKit
import Foundation

/// Artist-level edits: the artist's other printed names, which credits can
/// print in place of its own name.
@MainActor
final class ArtistEditor: Observable {
    let addCreditName:
        @Sendable (_ artistId: String, _ name: String) async throws -> Void
    let renameCreditName:
        @Sendable (_ artistId: String, _ key: String, _ name: String)
            async throws -> Void
    let removeCreditName:
        @Sendable (_ artistId: String, _ key: String) async throws -> Void

    init(
        addCreditName:
            @escaping @Sendable (String, String) async throws -> Void = {
                _,
                _ in
            },
        renameCreditName:
            @escaping @Sendable (String, String, String) async throws -> Void =
            { _, _, _ in },
        removeCreditName:
            @escaping @Sendable (String, String) async throws -> Void = {
                _,
                _ in
            }
    ) {
        self.addCreditName = addCreditName
        self.renameCreditName = renameCreditName
        self.removeCreditName = removeCreditName
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            addCreditName: {
                try await handle.addArtistCreditName(artistId: $0, name: $1)
            },
            renameCreditName: {
                try await handle.renameArtistCreditName(
                    artistId: $0,
                    key: $1,
                    name: $2
                )
            },
            removeCreditName: {
                try await handle.removeArtistCreditName(artistId: $0, key: $1)
            }
        )
    }
}
