import AppKit
import BaeKit
import Foundation

struct ExportFormatChoice {
    let title: String
    let extensionName: String?
    let selection: BridgeExportSelection

    @MainActor
    static func releaseChoices(
        presets: [BridgeExportPreset]
    ) -> [ExportFormatChoice] {
        var choices = [
            ExportFormatChoice(
                title: String(localized: "Original"),
                extensionName: nil,
                selection: .original
            )
        ]
        choices.append(
            contentsOf:
                presets
                .filter(\.appliesToRelease)
                .map {
                    ExportFormatChoice(
                        title: $0.name,
                        extensionName: $0.extension,
                        selection: .preset(presetId: $0.id)
                    )
                }
        )
        return choices
    }
}

struct ReleaseExportTarget {
    let targetDir: String
    let selection: BridgeExportSelection
}

/// Resolves the destination directory and format for a release export. The
/// format choices come from the export presets; the default selection from the
/// config. The destination is always chosen in a folder dialog, seeded with the
/// last-used output folder (`lastExportFolder`, per-device UI memory) and
/// written back after the user picks. Returns `nil` when the user picks no
/// folder, so the caller enqueues nothing. Shared by every export-trigger call
/// site.
enum ExportTarget {
    /// UserDefaults key for the last folder a release output was written to.
    /// Per-device UI convenience, not synced config — it only seeds the picker.
    private static let lastExportFolderKey = "lastExportFolder"

    @MainActor
    static func resolveRelease(config: Config) -> ReleaseExportTarget? {
        let choices = ExportFormatChoice.releaseChoices(
            presets: config.exportPresets
        )
        guard
            let selectedIndex = selectedFormatIndex(
                choices: choices,
                defaultSelection: config.defaultReleaseExportSelection
            )
        else {
            showDefaultFormatUnavailableAlert()
            return nil
        }

        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = String(localized: "Export Here")
        if let last = UserDefaults.standard.string(forKey: lastExportFolderKey),
            !last.isEmpty
        {
            panel.directoryURL = URL(fileURLWithPath: last)
        }
        let popup = makeFormatPopup(
            choices: choices,
            selectedIndex: selectedIndex
        )
        panel.accessoryView = formatAccessoryView(popup: popup)
        guard panel.runModal() == .OK, let url = panel.url else {
            return nil
        }
        let targetDir = url.path(percentEncoded: false)
        UserDefaults.standard.set(targetDir, forKey: lastExportFolderKey)
        return ReleaseExportTarget(
            targetDir: targetDir,
            selection: choices[popup.indexOfSelectedItem].selection
        )
    }

    @MainActor
    private static func selectedFormatIndex(
        choices: [ExportFormatChoice],
        defaultSelection: BridgeExportSelection
    ) -> Int? {
        choices.firstIndex { $0.selection == defaultSelection }
    }

    @MainActor
    private static func showDefaultFormatUnavailableAlert() {
        let alert = NSAlert()
        alert.messageText = String(localized: "Export Failed")
        alert.informativeText = String(localized: "Default format")
        alert.addButton(withTitle: String(localized: "OK"))
        alert.runModal()
    }

    @MainActor
    private static func makeFormatPopup(
        choices: [ExportFormatChoice],
        selectedIndex: Int
    ) -> NSPopUpButton {
        let popup = NSPopUpButton(
            frame: NSRect(x: 54, y: 5, width: 190, height: 24),
            pullsDown: false
        )
        popup.addItems(withTitles: choices.map(\.title))
        popup.selectItem(at: selectedIndex)
        return popup
    }

    @MainActor
    private static func formatAccessoryView(popup: NSPopUpButton) -> NSView {
        let accessoryContainer = NSView(
            frame: NSRect(x: 0, y: 0, width: 250, height: 34)
        )
        let label = NSTextField(labelWithString: String(localized: "Format"))
        label.frame = NSRect(x: 0, y: 7, width: 50, height: 20)
        accessoryContainer.addSubview(label)
        accessoryContainer.addSubview(popup)
        return accessoryContainer
    }
}
