import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

@Suite("SaveNameTokenField")
@MainActor
struct SaveNameTokenFieldTests {
    typealias Field = SaveNameTokenField<BridgeSaveFilenamePiece>

    /// A field in a window, holding `pieces`, with the patterns the store is
    /// handed recorded in order.
    @MainActor
    final class Bench {
        let coordinator = Field.Coordinator()
        let field: Field.TokenField
        let window: NSWindow
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
            coordinator.stored(pieces)
        }

        /// The field being typed in.
        func edit() throws -> NSTextView {
            window.makeFirstResponder(field)
            return try #require(field.currentEditor() as? NSTextView)
        }

        func settle() async {
            while !coordinator.isIdle {
                await Task.yield()
            }
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
        bench.coordinator.append(.artist)
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

    struct RefusedWrite: Error {}
}

@Suite("Save as names in a window")
@MainActor
struct SaveNameFieldLayoutTests {
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
                .compactMap { $0 as? NSTokenField }
            #expect(fields.count == 2)
            for field in fields {
                window.makeFirstResponder(field)
                try await SnapshotTestSupport.settle(host)
                #expect(field.currentEditor() != nil)
            }
        }
    }
}

extension SaveNameTokenField.Coordinator
where Piece == BridgeSaveFilenamePiece {
    /// Whether every pattern handed over has been stored or refused.
    var isIdle: Bool {
        !writing
    }
}
