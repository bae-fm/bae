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

        /// The button that ends the library, in a row and in the
        /// confirmation.
        var action: String {
            switch self {
            case .remove: String(localized: "Remove")
            case .delete: String(localized: "Delete")
            }
        }

        /// The menu item that asks to end the library.
        var menuItem: String {
            switch self {
            case .remove: String(localized: "Remove…")
            case .delete: String(localized: "Delete…")
            }
        }
    }

    /// A synced library stays in storage and on the person's other devices. A
    /// never-synced library has no other copy: it is deleted for good, while
    /// indexed audio files stay in the user's folders.
    static func message(syncs: Bool) -> String {
        guard syncs else {
            return String(
                localized:
                    "This library isn’t synced, so it will be deleted for good. Your audio files stay in their folders."
            )
        }
        return String(
            localized:
                "The library will remain in the cloud and on your other devices."
        )
    }

    /// The confirmation's question, naming the library.
    static func title(for library: BridgeLibrary) -> String {
        switch Ending(of: library) {
        case .remove:
            String(localized: "Remove \(library.name) from this Mac?")
        case .delete: String(localized: "Delete \(library.name)?")
        }
    }

    /// Whether the confirmation leads with a warning that changes this Mac
    /// hasn't sent will be lost: only when the bridge found some. A library
    /// that couldn't be counted says so in its message instead.
    static func warnsOfUnsent(_ loss: RemovalLoss) -> Bool {
        switch loss {
        case .counted(let holdsUnsent): holdsUnsent
        case .uncounted: false
        }
    }

    /// The confirmation's text under its title, for removing `library`, which
    /// isn't open, losing `loss`. A library that can't open is deleted from
    /// this Mac without leaving, so storage plays no part, and whether it has
    /// a cloud copy can't be read.
    static func message(for library: BridgeLibrary, losing loss: RemovalLoss)
        -> String
    {
        switch loss {
        case .counted:
            guard case .openable(let storage) = library.state else {
                preconditionFailure(
                    "only a library that opens has its unsent changes counted"
                )
            }
            return message(syncs: storage != nil)
        case .uncounted:
            return String(
                localized:
                    "This library can’t be opened, so any changes on this Mac that weren’t sent are lost. Its copy on this Mac is deleted; if the library is kept in the cloud, it stays there."
            )
        }
    }
}
