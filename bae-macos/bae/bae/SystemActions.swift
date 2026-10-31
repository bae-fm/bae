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

    /// Reveal in Finder, selected, the folder whose path crosses the bridge as
    /// `folder`, coven's bytes for it. The URL is made from the bytes
    /// themselves, never from text, so a folder whose name isn't UTF-8 is
    /// revealed too, and a folder that is a package is shown, not opened.
    static func revealInFinder(folder: Data) {
        NSWorkspace.shared.activateFileViewerSelecting([
            URL(filePathBytes: folder, isDirectory: true)
        ])
    }

    /// Replace the general pasteboard's contents with `value` as a string.
    static func copyToPasteboard(_ value: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(value, forType: .string)
    }
}
