import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

// The sheet caption's audio assignments: which references are listed, and
// where a bound one is changed.
extension ImportMappingTracksLayoutTests {
    /// With every reference bound, the summary's menu has a submenu per
    /// reference, each holding only that reference's choices.
    @Test("each bound reference has its own submenu of its own choices")
    func boundReferencesHaveTheirOwnSubmenus() throws {
        let sheet = assignmentSheet(secondAssigned: true)
        let references = sheet.referenceOptions
        let layout = ImportSheetBindingLayout(sheet: sheet)
        #expect(layout.summary == .submenus(references))
        #expect(layout.unbound.isEmpty)
        #expect(
            ImportSheetBindingChoice.choices(for: references[0])
                == [
                    .offered(fileId: "First.flac", isBound: true),
                    .offered(fileId: "Replacement.flac", isBound: false),
                    try refusal(of: "Wrong.mp3", in: references[0]),
                    try refusal(of: "Short.flac", in: references[0]),
                    .describesNothing(isBound: false),
                ]
        )
        #expect(
            ImportSheetBindingChoice.choices(for: references[1])
                == [
                    .offered(fileId: "Second.flac", isBound: true),
                    .describesNothing(isBound: false),
                ]
        )
    }

    /// The one reference with nothing bound is the one thing left to do, so
    /// it is the one row: the bound reference is not listed, and its choices
    /// sit directly in the summary's menu since it is the only bound one.
    @MainActor
    @Test("a partial CUE lists only its missing reference")
    func partialCueAssignments() async throws {
        let sheet = assignmentSheet(secondAssigned: false)
        let size = NSSize(width: 900, height: 180)
        try await withHostedSheet(sheet, size: size) { _, host in
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
            #expect(menus.count == 2, "the summary, then the missing row")
        }
        let references = sheet.referenceOptions
        let layout = ImportSheetBindingLayout(sheet: sheet)
        #expect(layout.summary == .choices(references[0]))
        #expect(layout.unbound == [references[1]])
        #expect(
            ImportSheetBindingChoice.choices(for: references[1])
                == [
                    .offered(fileId: "Second.flac", isBound: false),
                    .describesNothing(isBound: true),
                ]
        )
    }

    /// A file core refuses for `reference`, listed with core's reason.
    private func refusal(
        of fileId: String,
        in reference: BridgeSheetReferenceOptions
    ) throws -> ImportSheetBindingChoice {
        let option = try #require(
            reference.options.first { $0.fileId == fileId }
        )
        return .refused(
            fileId: fileId,
            reason: try #require(option.refusalLine)
        )
    }

    private func assignmentSheet(secondAssigned: Bool) -> BridgeSheetGroup {
        BridgeSheetGroup(
            sheetId: "Collection.cue",
            name: "Collection.cue",
            size: 2_048,
            localPath: Data("/tmp/source/Collection.cue".utf8),
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
            localPath: Data("/tmp/source/Collection.cue".utf8),
            bound: .describesFiles(audioFileCount: 2),
            referenceOptions: references,
            assignment: .disc(number: 1),
            discOptions: [1]
        )
        let size = NSSize(width: 900, height: 180)
        try await withHostedSheet(group, size: size) { _, host in
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
        }
    }
}

extension ImportMappingTracksLayoutTests {
    @MainActor
    private func withHostedSheet<Value>(
        _ sheet: BridgeSheetGroup,
        size: NSSize,
        _ body: (NSWindow, NSView) async throws -> Value
    ) async throws -> Value {
        try await SnapshotTestSupport.withHostedWindow(
            ImportSheetCaptionRow(
                sheet: sheet,
                evidence: [],
                showsDiscMenu: false,
                actions: actions(recording: MappingTrackActionRecorder())
            )
            .padding(20)
            .frame(
                width: size.width,
                height: size.height,
                alignment: .topLeading
            ),
            size: size
        ) {
            try await body($0, $1)
        }
    }
}
