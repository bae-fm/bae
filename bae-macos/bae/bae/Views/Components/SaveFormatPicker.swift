import AppKit
import BaeKit
import Observation

/// The Format pop-up of a save panel: the formats by name, then a separator
/// and Edit formats…, which hands off to `openFormats` and leaves the choice
/// where it was.
///
/// The pop-up follows the formats in config while the panel is open: an edit
/// made in Settings shows in it, and a format added there is chosen at once,
/// so closing Settings leaves the panel ready to save with it. `onChange`
/// hears the chosen format whenever the person picks one or a change in
/// config moves or renames it. Call `stop()` when the panel closes.
///
/// Each item is keyed by its preset's id, never by its title: a format name
/// is unique in core, but `NSPopUpButton.addItems(withTitles:)` keeps one item
/// per title and the id is what a save needs.
@MainActor
final class SaveFormatPicker: NSObject {
    let popup: NSPopUpButton
    private(set) var selectedPresetId: String

    private let configStore: ConfigStore
    private let openFormats: @MainActor () -> Void
    private let onChange: @MainActor (SaveFormatChoice) -> Void
    private var choices: [SaveFormatChoice]
    private var watching = true

    init(
        configStore: ConfigStore,
        selectedPresetId: String,
        frame: NSRect,
        openFormats: @escaping @MainActor () -> Void,
        onChange: @escaping @MainActor (SaveFormatChoice) -> Void
    ) {
        let choices = SaveFormatChoice.choices(
            presets: configStore.config.savePresets
        )
        precondition(
            choices.contains { $0.presetId == selectedPresetId },
            "the format a save starts with is one of the formats"
        )
        self.configStore = configStore
        self.selectedPresetId = selectedPresetId
        self.openFormats = openFormats
        self.onChange = onChange
        self.choices = choices
        popup = NSPopUpButton(frame: frame, pullsDown: false)
        super.init()
        popup.target = self
        popup.action = #selector(popupChanged(_:))
        rebuildItems()
        watch()
    }

    /// Stop following config: the panel is closed.
    func stop() {
        watching = false
    }

    private func rebuildItems() {
        popup.removeAllItems()
        for choice in choices {
            let item = NSMenuItem(
                title: choice.title,
                action: nil,
                keyEquivalent: ""
            )
            item.representedObject = choice.presetId
            popup.menu?.addItem(item)
        }
        popup.menu?.addItem(.separator())
        let edit = NSMenuItem(
            title: String(localized: "Edit formats…"),
            action: nil,
            keyEquivalent: ""
        )
        popup.menu?.addItem(edit)
        select(selectedPresetId)
    }

    private func select(_ presetId: String) {
        popup.select(
            popup.itemArray.first {
                $0.representedObject as? String == presetId
            }
        )
    }

    @objc
    private func popupChanged(_ sender: NSPopUpButton) {
        guard let presetId = sender.selectedItem?.representedObject as? String
        else {
            // Edit formats…: the choice stays where it was.
            select(selectedPresetId)
            openFormats()
            return
        }
        selectedPresetId = presetId
        onChange(selectedChoice)
    }

    private var selectedChoice: SaveFormatChoice {
        guard
            let choice = choices.first(where: {
                $0.presetId == selectedPresetId
            })
        else {
            preconditionFailure("the chosen format is one of the formats")
        }
        return choice
    }

    /// Follow config: choose a format that was just added, fall back to the
    /// last used when the chosen one is gone, and show renames.
    private func watch() {
        withObservationTracking {
            _ = configStore.config.savePresets
        } onChange: { [weak self] in
            Task { @MainActor in
                self?.presetsChanged()
            }
        }
    }

    private func presetsChanged() {
        guard watching else { return }
        defer { watch() }
        let before = Set(choices.map(\.presetId))
        let after = SaveFormatChoice.choices(
            presets: configStore.config.savePresets
        )
        if after.map(\.presetId) == choices.map(\.presetId)
            && after.map(\.title) == choices.map(\.title)
            && after.map(\.extensionName) == choices.map(\.extensionName)
        {
            return
        }
        choices = after
        if let added = after.last(where: { !before.contains($0.presetId) }) {
            selectedPresetId = added.presetId
        }
        else if !after.contains(where: { $0.presetId == selectedPresetId }) {
            selectedPresetId = configStore.config.lastSavePreset
        }
        rebuildItems()
        onChange(selectedChoice)
    }
}
