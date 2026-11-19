import AppKit
import BaeKit
import Testing

@testable import bae

/// Core's save choice as the pop-up reads it: each open read answers again
/// whenever the formats change or its selection moves, with the selected
/// format chosen while a row holds it, else the last used.
private final class FakeSaveChoice: @unchecked Sendable {
    private let lock = NSLock()
    private var presets: [BridgeSavePreset]
    private let lastUsed: String
    private var reads:
        [(selected: String, feed: ImportFeed<[BridgeSaveChoiceRow]>)] =
            []

    init(_ presets: [BridgeSavePreset], lastUsed: String) {
        self.presets = presets
        self.lastUsed = lastUsed
    }

    private func rows(selected: String) -> [BridgeSaveChoiceRow] {
        let chosen =
            presets.contains { $0.id == selected } ? selected : lastUsed
        return presets.map {
            BridgeSaveChoiceRow(preset: $0, chosen: $0.id == chosen)
        }
    }

    func set(_ presets: [BridgeSavePreset]) {
        lock.withLock {
            self.presets = presets
            for read in reads { read.feed.emit(rows(selected: read.selected)) }
        }
    }

    func outputs() -> Outputs {
        Outputs(
            newSavePreset: { _ in
                BridgeSavePreset(
                    id: "new",
                    name: "New",
                    codec: .flac(bitDepth: .sameAsSource)
                )
            },
            subscribeSaveChoice: { [self] selected in
                let feed = ImportFeed<[BridgeSaveChoiceRow]>()
                let index = lock.withLock {
                    reads.append((selected, feed))
                    feed.emit(rows(selected: selected))
                    return reads.count - 1
                }
                return SaveChoiceRead(
                    values: PulledValues(read: { try await feed.next() }),
                    select: { [self] id in
                        lock.withLock {
                            reads[index].selected = id
                            feed.emit(rows(selected: id))
                        }
                    }
                )
            }
        )
    }
}

@MainActor
@Suite("Save format picker")
struct SaveFormatPickerTests {
    private func preset(_ id: String, _ name: String) -> BridgeSavePreset {
        BridgeSavePreset(
            id: id,
            name: name,
            codec: .flac(bitDepth: .sameAsSource)
        )
    }

    /// What a picker reported, and a way to wait until it has reported
    /// enough.
    @MainActor
    private final class ChoiceLog {
        var ids: [String] = []
        private var waiter: (() -> Bool, CheckedContinuation<Void, Never>)?

        func record(_ choice: SaveFormatChoice) {
            ids.append(choice.presetId)
            if let (done, continuation) = waiter, done() {
                waiter = nil
                continuation.resume()
            }
        }

        /// Returns once the picker has reported `count` times.
        func untilReports(_ count: Int) async {
            if ids.count >= count { return }
            await withCheckedContinuation { continuation in
                waiter = (
                    { [unowned self] in ids.count >= count }, continuation
                )
            }
        }

        /// Returns once the last report is for `id`.
        func untilChosen(_ id: String) async {
            if ids.last == id { return }
            await withCheckedContinuation { continuation in
                waiter = ({ [unowned self] in ids.last == id }, continuation)
            }
        }
    }

    private struct Open {
        let core: FakeSaveChoice
        let outputs: Outputs
        let log: ChoiceLog
        let picker: SaveFormatPicker
    }

    /// A picker over a fake core holding `presets`, with `lastUsed` last used.
    private func open(
        _ presets: [BridgeSavePreset],
        lastUsed: String,
        openFormats: @escaping @MainActor () -> Void = {}
    ) -> Open {
        let core = FakeSaveChoice(presets, lastUsed: lastUsed)
        let outputs = core.outputs()
        let log = ChoiceLog()
        let picker = SaveFormatPicker(
            configStore: PreviewData.makeConfigStore(
                libraryFullWidth: false,
                savePresets: presets,
                lastSavePreset: presets.first { $0.id == lastUsed }
            ),
            outputs: outputs,
            frame: .zero,
            openFormats: openFormats,
            onChange: { log.record($0) }
        )
        return Open(core: core, outputs: outputs, log: log, picker: picker)
    }

    @Test("each format keeps its own item and the chosen one is selected")
    func eachFormatKeepsItsItem() async {
        let open = open(
            [preset("a", "FLAC"), preset("b", "Phone")],
            lastUsed: "b"
        )
        defer { open.picker.stop() }
        await open.log.untilReports(1)

        #expect(
            open.picker.popup.itemArray.compactMap {
                $0.representedObject as? String
            }
                == ["a", "b"]
        )
        #expect(
            open.picker.popup.selectedItem?.representedObject as? String == "b"
        )
    }

    @Test("the menu ends in a separator and Edit Formats")
    func menuEndsInEditFormats() {
        let open = open([preset("a", "FLAC")], lastUsed: "a")
        defer { open.picker.stop() }

        let items = open.picker.popup.itemArray
        #expect(items.count == 3)
        #expect(items[1].isSeparatorItem)
        #expect(items[2].title == String(localized: "Edit formats…"))
        #expect(items[2].representedObject == nil)
    }

    @Test("choosing Edit Formats opens them and keeps the choice")
    func editFormatsOpensAndKeepsTheChoice() async {
        var opened = 0
        let open = open(
            [preset("a", "FLAC"), preset("b", "Phone")],
            lastUsed: "a",
            openFormats: { opened += 1 }
        )
        defer { open.picker.stop() }
        await open.log.untilReports(1)

        let popup = open.picker.popup
        popup.selectItem(at: 3)
        _ = popup.target?.perform(popup.action, with: popup)

        #expect(opened == 1)
        #expect(open.picker.selectedPresetId == "a")
        #expect(popup.selectedItem?.representedObject as? String == "a")
    }

    @Test("picking a format reports it")
    func pickingReportsTheFormat() async {
        let open = open(
            [preset("a", "FLAC"), preset("b", "Phone")],
            lastUsed: "a"
        )
        defer { open.picker.stop() }
        await open.log.untilReports(1)

        let popup = open.picker.popup
        popup.selectItem(at: 1)
        _ = popup.target?.perform(popup.action, with: popup)
        await open.log.untilChosen("b")

        #expect(open.picker.selectedPresetId == "b")
    }

    @Test("a format added in Settings is chosen at once")
    func anAddedFormatIsChosen() async throws {
        let open = open([preset("a", "FLAC")], lastUsed: "a")
        defer { open.picker.stop() }

        open.core.set([preset("a", "FLAC"), preset("c", "Car")])
        _ = try await open.outputs.addSavePreset(preset("c", "Car"))
        await open.log.untilChosen("c")

        #expect(open.picker.selectedPresetId == "c")
        #expect(open.picker.popup.selectedItem?.title == "Car")
    }

    @Test("a format added before its row arrives is drawn once the row does")
    func anAddedFormatWaitsForItsRow() async throws {
        let open = open([preset("a", "FLAC")], lastUsed: "a")
        defer { open.picker.stop() }

        _ = try await open.outputs.addSavePreset(preset("c", "Car"))
        #expect(open.picker.selectedPresetId == "a")
        open.core.set([preset("a", "FLAC"), preset("c", "Car")])
        await open.log.untilChosen("c")

        #expect(open.picker.popup.selectedItem?.title == "Car")
    }

    @Test("a format added in Settings is chosen in every open panel")
    func anAddedFormatIsChosenInEveryPanel() async throws {
        let first = open([preset("a", "FLAC")], lastUsed: "a")
        let secondLog = ChoiceLog()
        let second = SaveFormatPicker(
            configStore: PreviewData.makeConfigStore(
                libraryFullWidth: false,
                savePresets: [preset("a", "FLAC")]
            ),
            outputs: first.outputs,
            frame: .zero,
            openFormats: {},
            onChange: { secondLog.record($0) }
        )
        defer {
            first.picker.stop()
            second.stop()
        }

        first.core.set([preset("a", "FLAC"), preset("c", "Car")])
        _ = try await first.outputs.addSavePreset(preset("c", "Car"))
        await first.log.untilChosen("c")
        await secondLog.untilChosen("c")

        #expect(first.picker.selectedPresetId == "c")
        #expect(second.selectedPresetId == "c")
    }

    @Test("a panel opened after a format was added starts with the last used")
    func aLaterPanelStartsWithTheLastUsed() async throws {
        let core = FakeSaveChoice(
            [preset("a", "FLAC"), preset("c", "Car")],
            lastUsed: "a"
        )
        let outputs = core.outputs()
        _ = try await outputs.addSavePreset(preset("c", "Car"))
        let picker = SaveFormatPicker(
            configStore: PreviewData.makeConfigStore(
                libraryFullWidth: false,
                savePresets: [preset("a", "FLAC"), preset("c", "Car")]
            ),
            outputs: outputs,
            frame: .zero,
            openFormats: {},
            onChange: { _ in }
        )
        defer { picker.stop() }

        #expect(picker.selectedPresetId == "a")
    }

    @Test("a closed panel ignores an added format")
    func aClosedPanelIgnoresAnAddition() async throws {
        let open = open(
            [preset("a", "FLAC"), preset("c", "Car")],
            lastUsed: "a"
        )
        open.picker.stop()

        _ = try await open.outputs.addSavePreset(preset("c", "Car"))

        #expect(open.picker.selectedPresetId == "a")
    }

    @Test("a rename in Settings shows in the menu and keeps the choice")
    func aRenameShows() async {
        let open = open(
            [preset("a", "FLAC"), preset("b", "Phone")],
            lastUsed: "b"
        )
        defer { open.picker.stop() }

        await open.log.untilReports(1)
        open.core.set([preset("a", "FLAC"), preset("b", "Car")])
        await open.log.untilReports(2)

        #expect(open.picker.selectedPresetId == "b")
        #expect(open.picker.popup.selectedItem?.title == "Car")
    }

    @Test("deleting the chosen format shows the last used")
    func deletingTheChosenFormatShowsTheLastUsed() async {
        let open = open(
            [preset("a", "FLAC"), preset("b", "Phone")],
            lastUsed: "a"
        )
        defer { open.picker.stop() }
        await open.log.untilReports(1)

        let popup = open.picker.popup
        popup.selectItem(at: 1)
        _ = popup.target?.perform(popup.action, with: popup)
        await open.log.untilChosen("b")
        open.core.set([preset("a", "FLAC")])
        await open.log.untilChosen("a")

        #expect(open.picker.selectedPresetId == "a")
        #expect(popup.selectedItem?.representedObject as? String == "a")
    }
}
