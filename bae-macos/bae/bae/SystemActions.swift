import AppKit

/// Thin wrappers over the AppKit calls bae triggers from menu items and buttons:
/// revealing a path in Finder and putting a string on the pasteboard. Centralized
/// so the call sites read as one intent instead of repeating the raw
/// `NSWorkspace` / `NSPasteboard` sequences.
enum SystemActions {
    /// Open Finder with the item at `path` selected.
    static func revealInFinder(path: String) {
        NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: path)
    }

    /// Replace the general pasteboard's contents with `value` as a string.
    static func copyToPasteboard(_ value: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(value, forType: .string)
    }
}
