import AppKit
import BaeKit
import Combine
import Foundation
import SwiftUI
import Testing

@testable import bae

@Suite("SaveNameField")
@MainActor
struct SaveNameFieldTests {
    typealias Field = SaveNameField<BridgeSaveFilenamePiece>

    /// A field in a window, holding `pieces`, with the patterns the store is
    /// handed recorded in order.
    @MainActor
    final class Bench {
        let coordinator = Field.Coordinator()
        let field: SaveNameFieldView
        let window: NSWindow
        /// The Add row's values.
        let adds = PassthroughSubject<BridgeSaveFilenameValue, Never>()
        var stored: [[BridgeSaveFilenamePiece]] = []

        init(_ pieces: [BridgeSaveFilenamePiece]) {
            field = Field.makeField(coordinator: coordinator)
            window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 400, height: 100),
                styleMask: [.titled],
                backing: .buffered,
                defer: false
            )
            field.frame = NSRect(x: 10, y: 10, width: 380, height: 30)
            window.contentView?.addSubview(field)
            coordinator.setPieces = { [self] pieces in stored.append(pieces) }
            coordinator.receive(adds.eraseToAnyPublisher())
            coordinator.stored(pieces)
        }

        /// The field being typed in.
        func edit() throws -> NSTextView {
            #expect(window.makeFirstResponder(field))
            return field
        }

        /// Wait until every pattern handed over is stored or refused.
        func settle() async {
            await coordinator.writes?.value
        }
    }

    static let defaultPieces: [BridgeSaveFilenamePiece] = [
        .value(value: .trackNumber), .text(text: " "), .value(value: .title),
    ]

    @Test("a stored pattern shows as chips and text kept exactly")
    func showsTheStoredPatternAsItIs() {
        let pieces: [BridgeSaveFilenamePiece] = [
            .text(text: "  "), .value(value: .artist),
            .text(text: " -   "), .value(value: .album), .text(text: "  "),
        ]
        let bench = Bench(pieces)
        #expect(bench.coordinator.pieces(in: bench.field) == pieces)
    }

    @Test("typing text beside a chip is stored as typed")
    func typedTextIsStoredAsTyped() async throws {
        let bench = Bench(Self.defaultPieces)
        let editor = try bench.edit()
        editor.setSelectedRange(
            NSRange(location: editor.string.count, length: 0)
        )
        editor.insertText(
            "  (live)  ",
            replacementRange: editor.selectedRange()
        )
        await bench.settle()
        #expect(
            bench.stored.last == [
                .value(value: .trackNumber), .text(text: " "),
                .value(value: .title), .text(text: "  (live)  "),
            ]
        )
    }

    @Test("Delete after a chip takes the chip out")
    func deleteAfterAChipRemovesIt() async throws {
        let bench = Bench(Self.defaultPieces)
        let editor = try bench.edit()
        // The editor holds the first chip, a space, the second chip.
        editor.setSelectedRange(NSRange(location: 1, length: 0))
        editor.doCommand(by: #selector(NSResponder.deleteBackward(_:)))
        await bench.settle()
        #expect(
            bench.stored.last == [
                .text(text: " "), .value(value: .title),
            ]
        )
    }

    @Test("Forward Delete before a chip takes the chip out")
    func forwardDeleteBeforeAChipRemovesIt() async throws {
        let bench = Bench(Self.defaultPieces)
        let editor = try bench.edit()
        editor.setSelectedRange(NSRange(location: 2, length: 0))
        editor.doCommand(by: #selector(NSResponder.deleteForward(_:)))
        await bench.settle()
        #expect(
            bench.stored.last == [
                .value(value: .trackNumber), .text(text: " "),
            ]
        )
    }

    @Test("a chip from the Add row lands after what is typed")
    func addedChipFollowsTheTypedText() async throws {
        let bench = Bench(Self.defaultPieces)
        let editor = try bench.edit()
        editor.setSelectedRange(
            NSRange(location: editor.string.count, length: 0)
        )
        editor.insertText(" - ", replacementRange: editor.selectedRange())
        await bench.settle()
        bench.adds.send(.artist)
        await bench.settle()
        #expect(
            bench.stored.last == [
                .value(value: .trackNumber), .text(text: " "),
                .value(value: .title), .text(text: " - "),
                .value(value: .artist),
            ]
        )
        #expect(!bench.field.isEditing)
    }

    @Test("a pattern that is stored while typing doesn't reset the field")
    func theStoredPatternDoesNotOverwriteWhatIsBeingTyped() async throws {
        let bench = Bench(Self.defaultPieces)
        let editor = try bench.edit()
        editor.setSelectedRange(
            NSRange(location: editor.string.count, length: 0)
        )
        editor.insertText("ab", replacementRange: editor.selectedRange())
        // The store shows the pattern as it was a keystroke ago.
        bench.coordinator.stored(
            Self.defaultPieces + [.text(text: "a")]
        )
        #expect(
            bench.coordinator.pieces(in: bench.field)
                == Self.defaultPieces + [.text(text: "ab")]
        )
    }

    @Test("a pattern that is stored while not typing is shown")
    func theStoredPatternIsShownWhenNotTyping() {
        let bench = Bench(Self.defaultPieces)
        let changed: [BridgeSaveFilenamePiece] = [
            .value(value: .artist), .text(text: " - "), .value(value: .title),
        ]
        bench.coordinator.stored(changed)
        #expect(bench.coordinator.pieces(in: bench.field) == changed)
    }

    @Test("a refused write leaves the field showing what is stored")
    func aRefusedWriteIsNotKept() async throws {
        let bench = Bench(Self.defaultPieces)
        var failures = 0
        bench.coordinator.setPieces = { _ in throw RefusedWrite() }
        bench.coordinator.fail = { _ in failures += 1 }
        let editor = try bench.edit()
        editor.setSelectedRange(
            NSRange(location: editor.string.count, length: 0)
        )
        editor.insertText("x", replacementRange: editor.selectedRange())
        await bench.settle()
        bench.window.makeFirstResponder(nil)
        await bench.settle()
        #expect(failures >= 1)
        #expect(bench.coordinator.pieces(in: bench.field) == Self.defaultPieces)
    }

    @Test(
        "once its write returns, a field not typed in draws the stored pattern"
    )
    func theFieldDrawsTheStoredPatternOnceItsWriteReturns() async throws {
        let bench = Bench(Self.defaultPieces)
        let editor = try bench.edit()
        editor.setSelectedRange(
            NSRange(location: editor.string.count, length: 0)
        )
        editor.insertText("x", replacementRange: editor.selectedRange())
        bench.window.makeFirstResponder(nil)
        await bench.settle()
        #expect(bench.stored.last == Self.defaultPieces + [.text(text: "x")])
        // The store has not shown the write yet, so the field draws what it
        // shows.
        #expect(bench.coordinator.pieces(in: bench.field) == Self.defaultPieces)
    }

    @Test("typing between two chips is stored between them")
    func typedTextBetweenChipsIsStoredBetweenThem() async throws {
        let bench = Bench(Self.defaultPieces)
        let editor = try bench.edit()
        // After the first chip, before the space.
        editor.setSelectedRange(NSRange(location: 1, length: 0))
        editor.insertText("-", replacementRange: editor.selectedRange())
        await bench.settle()
        #expect(
            bench.stored.last == [
                .value(value: .trackNumber), .text(text: "- "),
                .value(value: .title),
            ]
        )
    }

    @Test("a dragged chip lands as the chip where it is dropped")
    func aDraggedChipComesBackAsTheChip() async throws {
        let bench = Bench(Self.defaultPieces)
        let editor = try bench.edit()
        let pasteboard = NSPasteboard(name: .init("save-name-chip-test"))
        pasteboard.clearContents()
        // The second chip.
        editor.setSelectedRange(NSRange(location: 2, length: 1))
        #expect(
            bench.field.writablePasteboardTypes == [SaveNameFieldView.chipType]
        )
        #expect(
            bench.field.writeSelection(
                to: pasteboard,
                type: SaveNameFieldView.chipType
            )
        )
        editor.setSelectedRange(NSRange(location: 0, length: 0))
        #expect(
            bench.field.readSelection(
                from: pasteboard,
                type: SaveNameFieldView.chipType
            )
        )
        await bench.settle()
        #expect(
            bench.stored.last == [
                .value(value: .title), .value(value: .trackNumber),
                .text(text: " "), .value(value: .title),
            ]
        )
    }

    @Test("pasted text comes in as text on one line")
    func pastedTextIsPlainTextOnOneLine() async throws {
        let bench = Bench(Self.defaultPieces)
        let editor = try bench.edit()
        let pasteboard = NSPasteboard(name: .init("save-name-text-test"))
        pasteboard.clearContents()
        pasteboard.setString("a\nb", forType: .string)
        editor.setSelectedRange(NSRange(location: 3, length: 0))
        #expect(bench.field.readSelection(from: pasteboard, type: .string))
        await bench.settle()
        #expect(
            bench.stored.last == Self.defaultPieces + [.text(text: "a b")]
        )
    }

    @Test("VoiceOver reads each chip by its value's name")
    func aChipReadsAsItsValue() throws {
        let bench = Bench(Self.defaultPieces)
        let storage = try #require(bench.field.textStorage)
        let chip = try #require(
            storage.attribute(.attachment, at: 0, effectiveRange: nil)
                as? NSTextAttachment
        )
        let cell = try #require(chip.attachmentCell as? NSCell)
        #expect(cell.accessibilityLabel() == "Track №")
        #expect(bench.field.accessibilityValue() == "Track № Title")
        #expect(
            bench.field.accessibilityString(
                for: NSRange(location: 1, length: 2)
            )
                == " Title"
        )
    }

    struct RefusedWrite: Error {}
}

@Suite("Save as names in a window")
@MainActor
struct MacFileNameFieldLayoutTests {
    /// Clicking into a name field settles: the layout it causes comes to
    /// rest, rather than invalidating the hosting view on every pass.
    @Test("clicking into a name field leaves the layout at rest")
    func focusingANameFieldSettlesLayout() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            FormatsSettingsTab()
                .environment(PreviewData.configStore())
                .environment(Outputs.stub())
                .environment(UiStore()),
            size: NSSize(width: 690, height: 632)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let fields = SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? SaveNameFieldView }
            #expect(fields.count == 2)
            for field in fields {
                window.makeFirstResponder(field)
                try await SnapshotTestSupport.settle(host)
                #expect(field.isEditing)
            }
        }
    }
}
