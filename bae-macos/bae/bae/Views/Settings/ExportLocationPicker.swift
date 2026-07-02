import AppKit
import SwiftUI

/// Preferences control for the export-location setting: shows the current
/// destination — "Ask each time" or a fixed folder path — and lets the user pick
/// a folder or switch back to prompting each time. Writes through the `Exports`
/// service; the change round-trips back via a `configChanged` event into
/// `ConfigStore`, so the label updates reactively without an optimistic write.
struct ExportLocationPicker: View {
    let configStore: ConfigStore
    let setLocation: @Sendable (BridgeExportLocation) throws -> Void
    let showError: @MainActor (DisplayError) -> Void

    var body: some View {
        LabeledContent("Export location") {
            HStack {
                Text(currentLabel)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Button("Choose Folder...") { chooseFolder() }
                if case .fixed = configStore.config.exportLocation {
                    Button("Ask Each Time") { set(.askEachTime) }
                }
            }
        }
    }

    private var currentLabel: String {
        switch configStore.config.exportLocation {
        case .askEachTime:
            return String(localized: "Ask each time")
        case .fixed(let dir):
            return dir
        }
    }

    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = String(localized: "Choose")
        guard panel.runModal() == .OK, let url = panel.url else { return }
        set(.fixed(dir: url.path(percentEncoded: false)))
    }

    private func set(_ location: BridgeExportLocation) {
        do {
            try setLocation(location)
        }
        catch let error as BridgeError {
            showError(DisplayError(error))
        }
        catch {
            showError(DisplayError(line: error.localizedDescription))
        }
    }
}
