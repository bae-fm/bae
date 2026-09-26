import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

// The sheet caption's audio assignments: which references are listed, and
// how a bound one is changed through the summary's menu.
extension ImportMappingTracksLayoutTests {
    /// A bound reference is changed through the summary's menu, one submenu
    /// per reference, and each submenu commands only its own reference.
    @MainActor
    @Test(
        "the summary menu assigns and clears only the chosen reference",
        arguments: [0, 1]
    )
    func cueReferenceCommands(referenceIndex: Int) async throws {
        let recorder = MappingTrackActionRecorder()
        let size = NSSize(width: 900, height: 180)
        let (window, host) = hostSheet(
            assignmentSheet(secondAssigned: true),
            recorder: recorder,
            size: size
        )
        defer { withExtendedLifetime(window) {} }
        try await SnapshotTestSupport.settle(host)
        let buttons = SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSPopUpButton }
        try #require(buttons.count == 1, "the summary is the one menu")
        let button = buttons[0]
        #expect(button.isEnabled)
        let reference = referenceIndex == 0 ? "First.wav" : "Second.wav"
        let audio = referenceIndex == 0 ? "Replacement.flac" : "Second.flac"
        SnapshotTestSupport.populateMenu(button)
        let summary = try #require(button.menu)
        let references = summary.items.filter { $0.submenu != nil }
        try #require(references.count == 2, "one submenu per bound reference")
        #expect(references[0].title.hasPrefix("First.wav"))
        #expect(references[1].title.hasPrefix("Second.wav"))
        let menu = try #require(references[referenceIndex].submenu)
        if referenceIndex == 0 { try checkRefusedAudioChoices(menu) }
        let replacement = try #require(
            menu.items.first { $0.title == audio }
        )
        menu.performActionForItem(at: menu.index(of: replacement))
        try await Wait.until { recorder.sheetBindings.count == 1 }
        var expected = [
            SheetAssignmentChange(
                sheet: "Collection.cue",
                reference: reference,
                audio: audio
            )
        ]
        #expect(recorder.sheetBindings == expected)
        try await clearBinding(
            referenceIndex,
            through: button,
            recorder: recorder
        )
        expected.append(
            SheetAssignmentChange(
                sheet: "Collection.cue",
                reference: reference,
                audio: nil
            )
        )
        #expect(recorder.sheetBindings == expected)
    }

    /// Reopen the summary and clear one bound reference through its submenu.
    @MainActor
    private func clearBinding(
        _ referenceIndex: Int,
        through button: NSPopUpButton,
        recorder: MappingTrackActionRecorder
    ) async throws {
        SnapshotTestSupport.populateMenu(button)
        let reopened = try #require(button.menu).items.compactMap(\.submenu)
        try #require(reopened.count == 2)
        let menu = reopened[referenceIndex]
        let clear = try #require(
            menu.items.first {
                $0.title == coreString("ui.import.sheet.describes_nothing")
            }
        )
        menu.performActionForItem(at: menu.index(of: clear))
        try await Wait.until { recorder.sheetBindings.count == 2 }
    }

    /// The one reference with nothing bound is the one thing left to do, so
    /// it is the one row: the bound reference is not listed, and its choices
    /// sit directly in the summary's menu since it is the only bound one.
    @MainActor
    @Test("a partial CUE lists only its missing reference")
    func partialCueAssignments() async throws {
        let size = NSSize(width: 900, height: 180)
        let (window, host) = hostSheet(
            assignmentSheet(secondAssigned: false),
            recorder: MappingTrackActionRecorder(),
            size: size
        )
        try await SnapshotTestSupport.settle(host)
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        let text = try await SnapshotTestSupport.recognizedText(in: png)
            .map { $0.text.replacingOccurrences(of: " ", with: "") }
        #expect(text.carrying("Second.wav"))
        #expect(!text.carrying("First.wav"))
        #expect(!text.carrying("First.flac"))
        #expect(!text.carrying("Second.flac"))
        let menus = SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSPopUpButton }
        try #require(menus.count == 2, "the summary, then the missing row")
        SnapshotTestSupport.populateMenu(menus[0])
        let summary = try #require(menus[0].menu)
        #expect(summary.items.allSatisfy { $0.submenu == nil })
        #expect(
            summary.items.contains {
                $0.title == "Replacement.flac" && $0.isEnabled
            }
        )
        try checkRefusedAudioChoices(summary)
        SnapshotTestSupport.populateMenu(menus[1])
        let row = try #require(menus[1].menu)
        #expect(
            row.items.contains { $0.title == "Second.flac" && $0.isEnabled }
        )
        withExtendedLifetime(window) {}
    }

    private func assignmentSheet(secondAssigned: Bool) -> BridgeSheetGroup {
        BridgeSheetGroup(
            sheetId: "Collection.cue",
            name: "Collection.cue",
            size: 2_048,
            localPath: "/tmp/source/Collection.cue",
            bound: secondAssigned
                ? .describesFiles(audioFileCount: 2)
                : .unresolved(requested: ["Second.wav"]),
            referenceOptions: [
                BridgeSheetReferenceOptions(
                    fileReference: "First.wav",
                    fileId: "First.flac",
                    options: [
                        BridgeSheetBindingOption(
                            fileId: "First.flac",
                            offer: .offered
                        ),
                        BridgeSheetBindingOption(
                            fileId: "Replacement.flac",
                            offer: .offered
                        ),
                        BridgeSheetBindingOption(
                            fileId: "Wrong.mp3",
                            offer: .refusedCodec(codec: "MP3")
                        ),
                        BridgeSheetBindingOption(
                            fileId: "Short.flac",
                            offer: .refusedTiming
                        ),
                    ]
                ),
                BridgeSheetReferenceOptions(
                    fileReference: "Second.wav",
                    fileId: secondAssigned ? "Second.flac" : nil,
                    options: [
                        BridgeSheetBindingOption(
                            fileId: "Second.flac",
                            offer: .offered
                        )
                    ]
                ),
            ],
            assignment: secondAssigned ? .disc(number: 1) : .ignored,
            discOptions: secondAssigned ? [1] : []
        )
    }
}

extension ImportMappingTracksLayoutTests {
    /// A sheet whose every reference is bound has nothing left to do, so no
    /// reference is listed: the summary states the count, and its menu is
    /// where a binding is changed — whether or not the picker's choices
    /// have loaded.
    @MainActor
    @Test(
        "a fully bound multi-file CUE lists no reference",
        arguments: [true, false]
    )
    func boundCueReferencesAreNotListed(hasOffers: Bool) async throws {
        let references = ["First", "Second"]
            .map { name in
                BridgeSheetReferenceOptions(
                    fileReference: "\(name).wav",
                    fileId: "\(name).flac",
                    options: hasOffers
                        ? [
                            BridgeSheetBindingOption(
                                fileId: "\(name).flac",
                                offer: .offered
                            )
                        ] : []
                )
            }
        let group = BridgeSheetGroup(
            sheetId: "Collection.cue",
            name: "Collection.cue",
            size: 2_048,
            localPath: "/tmp/source/Collection.cue",
            bound: .describesFiles(audioFileCount: 2),
            referenceOptions: references,
            assignment: .disc(number: 1),
            discOptions: [1]
        )
        let size = NSSize(width: 900, height: 180)
        let (window, host) = hostSheet(
            group,
            recorder: MappingTrackActionRecorder(),
            size: size
        )
        try await SnapshotTestSupport.settle(host)
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        let text = try await SnapshotTestSupport.recognizedText(in: png)
            .map { $0.text.replacingOccurrences(of: " ", with: "") }
        #expect(text.carrying("Collection.cue"))
        #expect(!text.carrying("First.wav"))
        #expect(!text.carrying("First.flac"))
        #expect(!text.carrying("Second.wav"))
        #expect(!text.carrying("Second.flac"))
        let menus = SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSPopUpButton }
        try #require(menus.count == 1, "the summary is the one menu")
        #expect(menus[0].isEnabled)
        withExtendedLifetime(window) {}
    }
}

extension ImportMappingTracksLayoutTests {
    @MainActor
    private func hostSheet(
        _ sheet: BridgeSheetGroup,
        recorder: MappingTrackActionRecorder,
        size: NSSize
    ) -> (NSWindow, NSView) {
        SnapshotTestSupport.hostInWindow(
            ImportSheetCaptionRow(
                sheet: sheet,
                evidence: [],
                showsDiscMenu: false,
                actions: actions(recording: recorder)
            )
            .padding(20)
            .frame(
                width: size.width,
                height: size.height,
                alignment: .topLeading
            ),
            size: size
        )
    }
}

extension ImportMappingTracksLayoutTests {
    @MainActor
    private func checkRefusedAudioChoices(_ menu: NSMenu) throws {
        for refusedFile in ["Wrong.mp3", "Short.flac"] {
            let refused = try #require(
                menu.items.first { $0.title.contains(refusedFile) }
            )
            #expect(!refused.isEnabled)
        }
    }
}
