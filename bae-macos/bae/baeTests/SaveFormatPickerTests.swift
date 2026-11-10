import AppKit
import BaeKit
import Testing

@testable import bae

@MainActor
@Suite("Save format picker")
struct SaveFormatPickerTests {
    private func preset(_ id: String, _ name: String) -> BridgeSavePreset {
        BridgeSavePreset(
            id: id,
            name: name,
            codec: .flac(bitDepth: .sameAsSource),
            embedCover: true
        )
    }

    private func store(
        _ presets: [BridgeSavePreset],
        lastUsed: String
    ) -> ConfigStore {
        PreviewData.makeConfigStore(
            libraryFullWidth: false,
            savePresets: presets,
            lastSavePreset: lastUsed
        )
    }

    /// What a picker reported, and a way to wait for its next report.
    @MainActor
    private final class ChoiceLog {
        var ids: [String] = []
        private var waiter: CheckedContinuation<Void, Never>?

        func record(_ choice: SaveFormatChoice) {
            ids.append(choice.presetId)
            waiter?.resume()
            waiter = nil
        }

        /// Runs `change` and returns once the picker has reported what it did.
        func report(after change: @MainActor () -> Void) async {
            await withCheckedContinuation { continuation in
                waiter = continuation
                change()
            }
        }
    }

    private func picker(
        _ configStore: ConfigStore,
        selected: String,
        openFormats: @escaping @MainActor () -> Void = {},
        onChange: @escaping @MainActor (SaveFormatChoice) -> Void = { _ in }
    ) -> SaveFormatPicker {
        SaveFormatPicker(
            configStore: configStore,
            selectedPresetId: selected,
            frame: .zero,
            openFormats: openFormats,
            onChange: onChange
        )
    }

    @Test("each format keeps its own item and the chosen one is selected")
    func eachFormatKeepsItsItem() {
        let configStore = store(
            [preset("a", "FLAC"), preset("b", "Phone")],
            lastUsed: "b"
        )
        let picker = picker(configStore, selected: "b")
        defer { picker.stop() }

        #expect(
            picker.popup.itemArray.compactMap {
                $0.representedObject as? String
            }
                == ["a", "b"]
        )
        #expect(picker.popup.selectedItem?.representedObject as? String == "b")
    }

    @Test("the menu ends in a separator and Edit Formats")
    func menuEndsInEditFormats() {
        let configStore = store([preset("a", "FLAC")], lastUsed: "a")
        let picker = picker(configStore, selected: "a")
        defer { picker.stop() }

        let items = picker.popup.itemArray
        #expect(items.count == 3)
        #expect(items[1].isSeparatorItem)
        #expect(items[2].title == String(localized: "Edit formats…"))
        #expect(items[2].representedObject == nil)
    }

    @Test("choosing Edit Formats opens them and keeps the choice")
    func editFormatsOpensAndKeepsTheChoice() {
        let configStore = store(
            [preset("a", "FLAC"), preset("b", "Phone")],
            lastUsed: "a"
        )
        var opened = 0
        let picker = picker(
            configStore,
            selected: "a",
            openFormats: { opened += 1 }
        )
        defer { picker.stop() }

        picker.popup.selectItem(at: 3)
        _ = picker.popup.target?
            .perform(
                picker.popup.action,
                with: picker.popup
            )

        #expect(opened == 1)
        #expect(picker.selectedPresetId == "a")
        #expect(picker.popup.selectedItem?.representedObject as? String == "a")
    }

    @Test("picking a format reports it")
    func pickingReportsTheFormat() {
        let configStore = store(
            [preset("a", "FLAC"), preset("b", "Phone")],
            lastUsed: "a"
        )
        let log = ChoiceLog()
        let picker = picker(
            configStore,
            selected: "a",
            onChange: { log.record($0) }
        )
        defer { picker.stop() }

        picker.popup.selectItem(at: 1)
        _ = picker.popup.target?
            .perform(
                picker.popup.action,
                with: picker.popup
            )

        #expect(log.ids == ["b"])
        #expect(picker.selectedPresetId == "b")
    }

    @Test("a format added in Settings is chosen at once")
    func anAddedFormatIsChosen() async {
        let configStore = store([preset("a", "FLAC")], lastUsed: "a")
        let log = ChoiceLog()
        let picker = picker(
            configStore,
            selected: "a",
            onChange: { log.record($0) }
        )
        defer { picker.stop() }

        await log.report {
            configStore.config =
                store(
                    [preset("a", "FLAC"), preset("c", "Car")],
                    lastUsed: "a"
                )
                .config
        }

        #expect(picker.selectedPresetId == "c")
        #expect(picker.popup.selectedItem?.title == "Car")
    }

    @Test("a rename in Settings shows in the menu and keeps the choice")
    func aRenameShows() async {
        let configStore = store(
            [preset("a", "FLAC"), preset("b", "Phone")],
            lastUsed: "b"
        )
        let log = ChoiceLog()
        let picker = picker(
            configStore,
            selected: "b",
            onChange: { log.record($0) }
        )
        defer { picker.stop() }

        await log.report {
            configStore.config =
                store(
                    [preset("a", "FLAC"), preset("b", "Car")],
                    lastUsed: "b"
                )
                .config
        }

        #expect(picker.selectedPresetId == "b")
        #expect(picker.popup.selectedItem?.title == "Car")
    }

    @Test("deleting the chosen format falls back to the last used")
    func deletingTheChosenFormat() async {
        let configStore = store(
            [preset("a", "FLAC"), preset("b", "Phone")],
            lastUsed: "b"
        )
        let log = ChoiceLog()
        let picker = picker(
            configStore,
            selected: "b",
            onChange: { log.record($0) }
        )
        defer { picker.stop() }

        await log.report {
            configStore.config =
                store(
                    [preset("a", "FLAC")],
                    lastUsed: "a"
                )
                .config
        }

        #expect(picker.selectedPresetId == "a")
    }
}
