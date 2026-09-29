import AppKit
import BaeKit
import Foundation

/// One preset the user can pick in a save flow: its display name, the file
/// extension its codec produces (carried across the bridge on the preset), and
/// its id. Built from the configured presets, filtered to the relevant level.
struct SaveFormatChoice {
    let title: String
    let extensionName: String
    let presetId: String

    static func trackChoices(
        presets: [BridgeSavePreset]
    ) -> [SaveFormatChoice] {
        presets
            .filter(\.appliesToTrack)
            .map {
                SaveFormatChoice(
                    title: $0.name,
                    extensionName: $0.extension,
                    presetId: $0.id
                )
            }
    }

    static func releaseChoices(
        presets: [BridgeSavePreset]
    ) -> [SaveFormatChoice] {
        presets
            .filter(\.appliesToRelease)
            .map {
                SaveFormatChoice(
                    title: $0.name,
                    extensionName: $0.extension,
                    presetId: $0.id
                )
            }
    }
}

/// The format popup of a save panel. Each item is keyed by its preset's id,
/// never by its title: preset names need not be unique, and
/// `NSPopUpButton.addItems(withTitles:)` keeps only one item per title.
enum SaveFormatPopup {
    @MainActor
    static func make(
        choices: [SaveFormatChoice],
        selectedPresetId: String,
        frame: NSRect
    ) -> NSPopUpButton {
        let popup = NSPopUpButton(frame: frame, pullsDown: false)
        for choice in choices {
            let item = NSMenuItem(
                title: choice.title,
                action: nil,
                keyEquivalent: ""
            )
            item.representedObject = choice.presetId
            popup.menu?.addItem(item)
        }
        popup.select(
            popup.itemArray.first {
                $0.representedObject as? String == selectedPresetId
            }
        )
        return popup
    }

    /// The preset id of the selected item. Every item `make` builds carries
    /// one, and a popup with items always has one selected.
    @MainActor
    static func selectedPresetId(of popup: NSPopUpButton) -> String {
        guard let presetId = popup.selectedItem?.representedObject as? String
        else {
            preconditionFailure(
                "a save-format popup item carries its preset id"
            )
        }
        return presetId
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
/// write back `lastOutputFolder` (per-device UI memory, not synced config).
/// Returns `nil` when the user cancels, so the caller enqueues nothing.
enum OutputTarget {
    /// UserDefaults key for the last folder a release output was written to.
    /// Per-device UI convenience, not synced config — it only seeds the picker.
    private static let lastOutputFolderKey = "lastOutputFolder"

    /// Verbatim export: a plain folder dialog, no format anywhere. Returns the
    /// chosen directory.
    @MainActor
    static func resolveExportDir() -> String? {
        let panel = makeFolderPanel()
        guard panel.runModal() == .OK, let url = panel.url else {
            return nil
        }
        let dir = url.path(percentEncoded: false)
        UserDefaults.standard.set(dir, forKey: lastOutputFolderKey)
        return dir
    }

    /// Release save: a folder dialog with a preset-picker accessory (release
    /// presets, default `defaultReleaseSavePreset`). Returns the chosen folder
    /// plus preset id.
    @MainActor
    static func resolveReleaseSave(config: Config) -> ReleaseSaveTarget? {
        let choices = SaveFormatChoice.releaseChoices(
            presets: config.savePresets
        )
        guard
            choices.contains(where: {
                $0.presetId == config.defaultReleaseSavePreset
            })
        else {
            showDefaultFormatUnavailableAlert()
            return nil
        }

        let panel = makeFolderPanel()
        let popup = SaveFormatPopup.make(
            choices: choices,
            selectedPresetId: config.defaultReleaseSavePreset,
            frame: NSRect(x: 54, y: 5, width: 190, height: 24)
        )
        panel.accessoryView = formatAccessoryView(popup: popup)
        guard panel.runModal() == .OK, let url = panel.url else {
            return nil
        }
        let dir = url.path(percentEncoded: false)
        UserDefaults.standard.set(dir, forKey: lastOutputFolderKey)
        return ReleaseSaveTarget(
            targetDir: dir,
            presetId: SaveFormatPopup.selectedPresetId(of: popup)
        )
    }

    @MainActor
    private static func makeFolderPanel() -> NSOpenPanel {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = String(localized: "Export Here")
        if let last = UserDefaults.standard.string(forKey: lastOutputFolderKey),
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
