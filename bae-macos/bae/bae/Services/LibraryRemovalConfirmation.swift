import BaeKit
import Foundation

enum LibraryRemovalConfirmation {
    /// What follows the counts of changes this Mac hasn't sent.
    static var consequence: String {
        String(localized: "Removing the library from this Mac discards them.")
    }

    /// A synced library stays in storage, and this Mac leaves its members
    /// first, which needs storage. A never-synced library's catalog is gone,
    /// while indexed audio files stay in the user's folders.
    static func message(syncs: Bool) -> String {
        guard syncs else {
            return String(
                localized:
                    "This library has never been synced. Its catalog will be permanently deleted; audio files in your folders aren't deleted."
            )
        }
        return String(
            localized:
                "This Mac leaves the library, then its copy is deleted. Removing needs a connection to your storage. The library will remain in storage and on your other devices."
        )
    }

    /// The confirmation for removing `library`, which isn't open, losing
    /// `loss`. A library that can't open is deleted from this Mac without
    /// leaving, so storage plays no part, and whether it has a cloud copy
    /// can't be read.
    static func text(for library: BridgeLibrary, losing loss: RemovalLoss)
        -> String
    {
        switch loss {
        case .unsent(let unsent):
            guard case .openable(let storage) = library.state else {
                preconditionFailure(
                    "only a library that opens has its unsent changes counted"
                )
            }
            return unsent.confirmation(
                consequence: consequence,
                message: message(syncs: storage != nil)
            )
        case .uncounted:
            return String(
                localized:
                    "This library can't be opened, so any changes on this Mac that weren't sent are lost. Its copy on this Mac is deleted; if the library is kept in storage, it stays there."
            )
        }
    }
}
