import AppKit
import Foundation

/// Resolves the destination directory for a release export from the
/// export-location setting: a fixed folder returns straight away; "ask each
/// time" opens one `NSOpenPanel`. Returns `nil` when the user picks no folder,
/// so the caller enqueues nothing. Shared by every export-trigger call site.
enum ExportTarget {
    @MainActor
    static func resolve(_ location: BridgeExportLocation) -> String? {
        switch location {
        case .fixed(let dir):
            return dir
        case .askEachTime:
            let panel = NSOpenPanel()
            panel.canChooseDirectories = true
            panel.canChooseFiles = false
            panel.canCreateDirectories = true
            panel.prompt = String(localized: "Export Here")
            guard panel.runModal() == .OK, let url = panel.url else {
                return nil
            }
            return url.path(percentEncoded: false)
        }
    }
}
