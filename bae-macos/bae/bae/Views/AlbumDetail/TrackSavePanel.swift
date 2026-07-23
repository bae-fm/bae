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

/// A track-save format choice paired with the stem its preset's pattern
/// suggests. Built together in one pass, so the stem is never absent — the
/// track panel and its format delegate read it directly, no lookup or fallback.
/// (The release flow doesn't pre-render stems, so this pairing is panel-local
/// rather than a field on the shared `SaveFormatChoice`.)
struct TrackSaveChoice {
    let choice: SaveFormatChoice
    let suggestedStem: String
}

/// Builds the `NSSavePanel` for a track save, with its format-picker accessory
/// wired to a delegate that keeps the extension and the suggested stem in sync
/// as the preset changes.
enum TrackSavePanel {
    /// Build the save panel for a track save, seeded from the default preset's
    /// suggestion; the caller runs the panel and reads `formatPopup` for the
    /// chosen preset. The delegate must be retained for the modal's lifetime.
    @MainActor
    static func make(
        saveChoices: [TrackSaveChoice],
        selectedIndex: Int
    ) -> SaveFilePanel {
        let selected = saveChoices[selectedIndex]
        let stem = selected.suggestedStem

        let panel = NSSavePanel()
        panel.nameFieldStringValue = "\(stem).\(selected.choice.extensionName)"
        panel.canCreateDirectories = true

        // Format picker accessory: swaps extension and re-suggests the stem on
        // change (the delegate replaces it only when the user hasn't edited it).
        let formatDelegate = SaveFormatDelegate(
            panel: panel,
            saveChoices: saveChoices,
            lastSuggestion: stem
        )
        let formatPopup = NSPopUpButton(
            frame: NSRect(x: 0, y: 0, width: 200, height: 24),
            pullsDown: false
        )
        formatPopup.addItems(withTitles: saveChoices.map(\.choice.title))
        formatPopup.selectItem(at: selectedIndex)
        formatPopup.target = formatDelegate
        formatPopup.action = #selector(SaveFormatDelegate.formatChanged(_:))

        let accessoryContainer = NSView(
            frame: NSRect(x: 0, y: 0, width: 250, height: 34)
        )
        let label = NSTextField(labelWithString: String(localized: "Format:"))
        label.frame = NSRect(x: 0, y: 7, width: 50, height: 20)
        formatPopup.frame = NSRect(x: 54, y: 5, width: 190, height: 24)
        accessoryContainer.addSubview(label)
        accessoryContainer.addSubview(formatPopup)
        panel.accessoryView = accessoryContainer

        if let type = UTType(filenameExtension: selected.choice.extensionName) {
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
/// format change it swaps the filename extension and content type, and
/// re-suggests the stem from the newly selected preset's pattern — but only when
/// the user hasn't edited the stem away from the last suggestion. Each choice
/// carries its pre-rendered stem (async work can't run during `runModal`), so
/// the suggestion is always present.
@MainActor
class SaveFormatDelegate: NSObject {
    weak var panel: NSSavePanel?
    let saveChoices: [TrackSaveChoice]
    /// The stem the panel currently shows as an unedited suggestion. When the
    /// visible stem still equals this, the user hasn't typed over it, so a
    /// format change is free to replace it with the new preset's suggestion.
    var lastSuggestion: String

    init(
        panel: NSSavePanel,
        saveChoices: [TrackSaveChoice],
        lastSuggestion: String
    ) {
        self.panel = panel
        self.saveChoices = saveChoices
        self.lastSuggestion = lastSuggestion
    }

    @objc
    func formatChanged(_ sender: NSPopUpButton) {
        guard let panel else {
            return
        }
        let selected = saveChoices[sender.indexOfSelectedItem]
        let currentStem =
            (panel.nameFieldStringValue as NSString).deletingPathExtension
        var stem = currentStem
        if currentStem == lastSuggestion {
            stem = selected.suggestedStem
            lastSuggestion = selected.suggestedStem
        }

        panel.nameFieldStringValue = "\(stem).\(selected.choice.extensionName)"
        if let type = UTType(filenameExtension: selected.choice.extensionName) {
            panel.allowedContentTypes = [type]
        }
    }
}
