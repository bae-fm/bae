import BaeKit
import Foundation

enum LibraryRemovalConfirmation {
    /// What ends a library on this Mac: a synced library is removed, and a
    /// never-synced one, which exists only here, is deleted.
    enum Ending {
        case remove
        case delete

        init(syncs: Bool) {
            self = syncs ? .remove : .delete
        }

        /// A library that can't open is removed: whether it has a cloud copy
        /// can't be read.
        init(of library: BridgeLibrary) {
            if case .openable(let storage) = library.state {
                self.init(syncs: storage != nil)
            }
            else {
                self = .remove
            }
        }

        /// The confirmation's question.
        var title: String {
            switch self {
            case .remove: String(localized: "Remove library from this Mac?")
            case .delete: String(localized: "Delete library?")
            }
        }

        /// The button that ends the library, in a row and in the
        /// confirmation.
        var action: String {
            switch self {
            case .remove: String(localized: "Remove")
            case .delete: String(localized: "Delete")
            }
        }
    }

    /// What follows the counts of changes this Mac hasn't sent.
    static var consequence: String {
        String(localized: "Removing the library from this Mac discards them.")
    }

    /// A synced library stays in storage and on the person's other devices. A
    /// never-synced library has no other copy: it is deleted for good, while
    /// indexed audio files stay in the user's folders.
    static func message(syncs: Bool) -> String {
        guard syncs else {
            return String(
                localized:
                    "This library isn't synced, so it will be deleted for good. Your audio files stay in their folders."
            )
        }
        return String(
            localized:
                "The library will remain in storage and on your other devices."
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
