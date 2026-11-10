import AppKit
import BaeKit
import Foundation
import UniformTypeIdentifiers

/// The pieces of a configured track-save panel: the panel itself and the
/// format picker the caller reads the chosen preset from.
struct SaveFilePanel {
    let savePanel: NSSavePanel
    let picker: SaveFormatPicker
}

/// Builds the `NSSavePanel` for a track save, with its format picker
/// accessory swapping the extension as the format changes. The name never
/// depends on the format, so a format change leaves the stem alone.
enum TrackSavePanel {
    /// Build the save panel for a track save, seeded with `suggestedStem`, which
    /// core renders from the library's one filename pattern, and the format
    /// the last save used; the caller shows the panel and reads `picker` for
    /// the chosen preset, and stops it when the panel closes.
    @MainActor
    static func make(
        suggestedStem stem: String,
        configStore: ConfigStore,
        openFormats: @escaping @MainActor () -> Void
    ) -> SaveFilePanel {
        let panel = NSSavePanel()
        panel.canCreateDirectories = true

        let picker = SaveFormatPicker(
            configStore: configStore,
            selectedPresetId: configStore.config.lastSavePreset,
            frame: NSRect(x: 54, y: 5, width: 190, height: 24),
            openFormats: openFormats,
            onChange: { [weak panel] choice in
                // The stem, suggested or typed, stays as it is.
                guard let panel else { return }
                let stem = (panel.nameFieldStringValue as NSString)
                    .deletingPathExtension
                panel.nameFieldStringValue = "\(stem).\(choice.extensionName)"
                if let type = UTType(filenameExtension: choice.extensionName) {
                    panel.allowedContentTypes = [type]
                }
            }
        )
        let selected =
            SaveFormatChoice.choices(
                presets: configStore.config.savePresets
            )
            .first { $0.presetId == picker.selectedPresetId }
        guard let selected else {
            preconditionFailure("the selected preset is one of the choices")
        }
        panel.nameFieldStringValue = "\(stem).\(selected.extensionName)"
        if let type = UTType(filenameExtension: selected.extensionName) {
            panel.allowedContentTypes = [type]
        }

        let accessoryContainer = NSView(
            frame: NSRect(x: 0, y: 0, width: 250, height: 34)
        )
        let label = NSTextField(labelWithString: String(localized: "Format:"))
        label.frame = NSRect(x: 0, y: 7, width: 50, height: 20)
        accessoryContainer.addSubview(label)
        accessoryContainer.addSubview(picker.popup)
        panel.accessoryView = accessoryContainer

        return SaveFilePanel(savePanel: panel, picker: picker)
    }
}
