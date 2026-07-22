import AppKit
import BaeKit
import Foundation

/// One preset the user can pick in a save flow: its display name, the file
/// extension its codec produces (carried across the bridge on the preset), and
/// its id. Built from the configured presets, filtered to the relevant level.
struct ExportFormatChoice {
    let title: String
    let extensionName: String
    let presetId: String

    static func trackChoices(
        presets: [BridgeExportPreset]
    ) -> [ExportFormatChoice] {
        presets
            .filter(\.appliesToTrack)
            .map {
                ExportFormatChoice(
                    title: $0.name,
                    extensionName: $0.extension,
                    presetId: $0.id
                )
            }
    }

    static func releaseChoices(
        presets: [BridgeExportPreset]
    ) -> [ExportFormatChoice] {
        presets
            .filter(\.appliesToRelease)
            .map {
                ExportFormatChoice(
                    title: $0.name,
                    extensionName: $0.extension,
                    presetId: $0.id
                )
            }
    }
}

/// A resolved release-save destination: the chosen folder plus the preset to
/// render with.
struct ReleaseSaveTarget {
    let targetDir: String
    let presetId: String
}

/// Destination pickers for release-level output. Export chooses a folder only
/// (verbatim, no format); save chooses a folder plus a preset. Both seed and
/// write back `lastExportFolder` (per-device UI memory, not synced config).
/// Returns `nil` when the user cancels, so the caller enqueues nothing.
enum ExportTarget {
    /// UserDefaults key for the last folder a release output was written to.
    /// Per-device UI convenience, not synced config — it only seeds the picker.
    private static let lastExportFolderKey = "lastExportFolder"

    /// Verbatim export: a plain folder dialog, no format anywhere. Returns the
    /// chosen directory.
    @MainActor
    static func resolveExportDir() -> String? {
        let panel = makeFolderPanel()
        guard panel.runModal() == .OK, let url = panel.url else {
            return nil
        }
        let dir = url.path(percentEncoded: false)
        UserDefaults.standard.set(dir, forKey: lastExportFolderKey)
        return dir
    }

    /// Release save: a folder dialog with a preset-picker accessory (release
    /// presets, default `defaultReleaseSavePreset`). Returns the chosen folder
    /// plus preset id.
    @MainActor
    static func resolveReleaseSave(config: Config) -> ReleaseSaveTarget? {
        let choices = ExportFormatChoice.releaseChoices(
            presets: config.exportPresets
        )
        guard
            let selectedIndex = choices.firstIndex(where: {
                $0.presetId == config.defaultReleaseSavePreset
            })
        else {
            showDefaultFormatUnavailableAlert()
            return nil
        }

        let panel = makeFolderPanel()
        let popup = makeFormatPopup(
            choices: choices,
            selectedIndex: selectedIndex
        )
        panel.accessoryView = formatAccessoryView(popup: popup)
        guard panel.runModal() == .OK, let url = panel.url else {
            return nil
        }
        let dir = url.path(percentEncoded: false)
        UserDefaults.standard.set(dir, forKey: lastExportFolderKey)
        return ReleaseSaveTarget(
            targetDir: dir,
            presetId: choices[popup.indexOfSelectedItem].presetId
        )
    }

    @MainActor
    private static func makeFolderPanel() -> NSOpenPanel {
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
        return panel
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
