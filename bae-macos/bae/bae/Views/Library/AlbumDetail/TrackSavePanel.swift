import AppKit
import Foundation
import UniformTypeIdentifiers

/// The pieces of a configured track-save panel: the panel itself, the format
/// popup the caller reads the chosen preset from, and the delegate that must be
/// retained for the modal's lifetime.
struct SaveFilePanel {
    let savePanel: NSSavePanel
    let formatPopup: NSPopUpButton
    let formatDelegate: SaveFormatDelegate
}

/// Builds the `NSSavePanel` for a track save, with its format-picker accessory
/// wired to a delegate that swaps the extension as the preset changes. The
/// name never depends on the format, so a format change leaves the stem alone.
enum TrackSavePanel {
    /// Build the save panel for a track save, seeded with `suggestedStem`, which
    /// core renders from the library's one filename pattern; the caller runs
    /// the panel and reads `formatPopup` for the chosen preset. The delegate
    /// must be retained for the modal's lifetime.
    @MainActor
    static func make(
        suggestedStem stem: String,
        choices: [SaveFormatChoice],
        selectedPresetId: String
    ) -> SaveFilePanel {
        guard
            let selected = choices.first(where: {
                $0.presetId == selectedPresetId
            })
        else {
            preconditionFailure("the selected preset is one of the choices")
        }

        let panel = NSSavePanel()
        panel.nameFieldStringValue = "\(stem).\(selected.extensionName)"
        panel.canCreateDirectories = true

        // Format picker accessory: swaps the extension on change.
        let formatDelegate = SaveFormatDelegate(panel: panel, choices: choices)
        let formatPopup = SaveFormatPopup.make(
            choices: choices,
            selectedPresetId: selectedPresetId,
            frame: NSRect(x: 54, y: 5, width: 190, height: 24)
        )
        formatPopup.target = formatDelegate
        formatPopup.action = #selector(SaveFormatDelegate.formatChanged(_:))

        let accessoryContainer = NSView(
            frame: NSRect(x: 0, y: 0, width: 250, height: 34)
        )
        let label = NSTextField(labelWithString: String(localized: "Format:"))
        label.frame = NSRect(x: 0, y: 7, width: 50, height: 20)
        accessoryContainer.addSubview(label)
        accessoryContainer.addSubview(formatPopup)
        panel.accessoryView = accessoryContainer

        if let type = UTType(filenameExtension: selected.extensionName) {
            panel.allowedContentTypes = [type]
        }

        return SaveFilePanel(
            savePanel: panel,
            formatPopup: formatPopup,
            formatDelegate: formatDelegate
        )
    }
}

/// Target-action handler for the format popup in the track save panel. On a
/// format change it swaps the filename extension and content type; the stem,
/// suggested or typed, stays as it is.
@MainActor
class SaveFormatDelegate: NSObject {
    weak var panel: NSSavePanel?
    let choices: [SaveFormatChoice]

    init(panel: NSSavePanel, choices: [SaveFormatChoice]) {
        self.panel = panel
        self.choices = choices
    }

    @objc
    func formatChanged(_ sender: NSPopUpButton) {
        guard let panel else {
            return
        }
        let presetId = SaveFormatPopup.selectedPresetId(of: sender)
        guard let selected = choices.first(where: { $0.presetId == presetId })
        else {
            preconditionFailure("the popup offers only the panel's choices")
        }
        let stem = (panel.nameFieldStringValue as NSString)
            .deletingPathExtension

        panel.nameFieldStringValue = "\(stem).\(selected.extensionName)"
        if let type = UTType(filenameExtension: selected.extensionName) {
            panel.allowedContentTypes = [type]
        }
    }
}
