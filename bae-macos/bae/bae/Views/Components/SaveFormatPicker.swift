import AppKit
import BaeKit
import Combine
import Observation
import os.log

private let logger = Logger.bae("SaveFormatPicker")

/// The Format pop-up of a save panel: the formats by name, then a separator
/// and Edit formats…, which hands off to `openFormats` and leaves the choice
/// where it was.
///
/// The pop-up draws one live read of core: the formats in menu order, with
/// the one the pop-up shows marked chosen, which is the format the person
/// selected while a format row holds it, else the format the last save used.
/// The picker holds only the selection, which it passes to the read: the
/// format the person picks, or the format added in Settings, which every open
/// panel selects when it hears the add. A selection whose row has not arrived
/// shows the last used format until it does. `onChange` hears the chosen
/// format each time the read answers. Call `stop()` when the panel closes.
///
/// Each item is keyed by its preset's id, never by its title: a format name
/// is unique in core, but `NSPopUpButton.addItems(withTitles:)` keeps one item
/// per title and the id is what a save needs.
@MainActor
final class SaveFormatPicker: NSObject {
    let popup: NSPopUpButton

    private let openFormats: @MainActor () -> Void
    private let onChange: @MainActor (SaveFormatChoice) -> Void
    private let choiceRead: SaveChoiceRead?
    private var choices: [SaveFormatChoice]
    private var chosenIndex: Int
    private var subscription:
        ScreenSubscription<PulledValues<[BridgeSaveChoiceRow]>>?
    private var addedSubscription: AnyCancellable?

    /// The id of the format the panel saves with: the one it draws chosen.
    var selectedPresetId: String { selectedChoice.presetId }

    /// The format the panel saves with, as the pop-up draws it.
    var selectedChoice: SaveFormatChoice { choices[chosenIndex] }

    /// Starts with the last used format alone, as config holds it, until the
    /// read answers with every format.
    init(
        configStore: ConfigStore,
        outputs: Outputs,
        frame: NSRect,
        openFormats: @escaping @MainActor () -> Void,
        onChange: @escaping @MainActor (SaveFormatChoice) -> Void
    ) {
        let last = configStore.config.lastSavePreset
        self.openFormats = openFormats
        self.onChange = onChange
        choices = SaveFormatChoice.choices(presets: [last])
        chosenIndex = 0
        choiceRead = Self.open { try outputs.subscribeSaveChoice(last.id) }
        popup = NSPopUpButton(frame: frame, pullsDown: false)
        super.init()
        popup.target = self
        popup.action = #selector(popupChanged(_:))
        rebuildItems()
        follow(outputs)
    }

    /// Stop following core: the panel is closed.
    func stop() {
        subscription = nil
        addedSubscription = nil
    }

    private static func open<Read>(_ make: () throws -> Read) -> Read? {
        do {
            return try make()
        }
        catch is CancellationError {
            return nil
        }
        catch {
            logger.warning(
                "a save panel could not read the formats: \(String(reflecting: error))"
            )
            return nil
        }
    }

    private func follow(_ outputs: Outputs) {
        if let choiceRead {
            subscription = ScreenSubscription(
                choiceRead.values,
                onValue: { [weak self] rows in self?.draw(rows) },
                onError: Self.log
            )
        }
        // The add returns on any thread, so the event hops to the main queue.
        addedSubscription = outputs.addedSavePresets
            .receive(on: DispatchQueue.main)
            .sink { [weak self] id in
                MainActor.assumeIsolated { self?.select(id) }
            }
    }

    private static func log(_ error: any Error) {
        logger.warning(
            "a save panel's read failed: \(String(reflecting: error))"
        )
    }

    /// Ask the choice read for the format `presetId`.
    private func select(_ presetId: String) {
        do {
            try choiceRead?.select(presetId)
        }
        catch {
            Self.log(error)
        }
    }

    private func draw(_ rows: [BridgeSaveChoiceRow]) {
        choices = SaveFormatChoice.choices(presets: rows.map(\.preset))
        guard let index = rows.firstIndex(where: \.chosen) else {
            preconditionFailure("the read marks one format chosen")
        }
        chosenIndex = index
        rebuildItems()
        onChange(selectedChoice)
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
        drawChosen()
    }

    private func drawChosen() {
        popup.selectItem(at: chosenIndex)
    }

    @objc
    private func popupChanged(_ sender: NSPopUpButton) {
        guard let presetId = sender.selectedItem?.representedObject as? String
        else {
            // Edit formats…: the choice stays where it was.
            drawChosen()
            openFormats()
            return
        }
        select(presetId)
    }
}
