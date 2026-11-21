import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

// The sheet caption's audio assignments: which references are listed, and
// where a bound one is changed. Core splits the references into the bound
// and the unbound, and the menus the caption draws are AppKit pop-up
// buttons; the words are the `import-mapping-cue-wide` shot's.
extension ImportMappingTracksLayoutTests {
    /// With every reference bound, the summary's menu has a submenu per
    /// reference, each holding only that reference's choices.
    @Test("each bound reference has its own submenu of its own choices")
    func boundReferencesHaveTheirOwnSubmenus() throws {
        let references = assignmentReferences(secondAssigned: true)
        #expect(
            ImportSheetBindingChoice.choices(
                for: references[0],
                offersNone: true
            )
                == [
                    .offered(
                        fileId: "First.flac",
                        name: "First.flac",
                        isBound: true
                    ),
                    .offered(
                        fileId: "Replacement.flac",
                        name: "Replacement.flac",
                        isBound: false
                    ),
                    try refusal(of: "Wrong.mp3", in: references[0]),
                    try refusal(of: "Short.flac", in: references[0]),
                    .describesNothing(isBound: false),
                ]
        )
        #expect(
            ImportSheetBindingChoice.choices(
                for: references[1],
                offersNone: true
            )
                == [
                    .offered(
                        fileId: "Second.flac",
                        name: "Second.flac",
                        isBound: true
                    ),
                    .describesNothing(isBound: false),
                ]
        )
    }

    /// The one reference with nothing bound is the one thing left to do, so
    /// it is the caption's second menu: the bound reference is not listed, and
    /// its choices sit directly in the summary's menu since it is the only
    /// bound one.
    @MainActor
    @Test("a partial CUE lists only its missing reference")
    func partialCueAssignments() async throws {
        let sheet = assignmentSheet(secondAssigned: false)
        let size = NSSize(width: 900, height: 180)
        try await withHostedSheet(sheet, size: size) { _, host in
            try await SnapshotTestSupport.settle(host)
            let menus = HostedMenus.controls(in: host)
            #expect(
                menus.count == 3,
                "the place pop-up, the summary, then the missing reference"
            )
        }
        let references = assignmentReferences(secondAssigned: false)
        #expect(
            ImportSheetBindingChoice.choices(
                for: references[1],
                offersNone: true
            )
                == [
                    .offered(
                        fileId: "Second.flac",
                        name: "Second.flac",
                        isBound: false
                    ),
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
            name: option.name,
            reason: try #require(option.offer.refusalLine)
        )
    }

    private func assignmentSheet(secondAssigned: Bool) -> BridgeSheetGroup {
        let references = assignmentReferences(secondAssigned: secondAssigned)
        return BridgeSheetGroup(
            sheetId: "Collection.cue",
            name: "Collection.cue",
            bound: secondAssigned
                ? .describesFiles(audioFileCount: 2)
                : .unresolved(requested: ["Second.wav"]),
            boundAudio: secondAssigned
                ? .several(references: references)
                : .one(reference: references[0]),
            unboundReferences: secondAssigned ? nil : [references[1]],
            assigned: secondAssigned ? .disc(1) : nil,
            sideChoices: secondAssigned ? BridgeSheetSideChoice.discs(1) : []
        )
    }

    /// The sheet's two references: the first bound, the second bound only
    /// when `secondAssigned`.
    private func assignmentReferences(
        secondAssigned: Bool
    ) -> [BridgeSheetReferenceOptions] {
        [
            BridgeSheetReferenceOptions(
                fileReference: "First.wav",
                bound: BridgeMappingContainer(
                    fileId: "First.flac",
                    name: "First.flac"
                ),
                options: [
                    BridgeSheetBindingOption(
                        fileId: "First.flac",
                        name: "First.flac",
                        offer: .offered
                    ),
                    BridgeSheetBindingOption(
                        fileId: "Replacement.flac",
                        name: "Replacement.flac",
                        offer: .offered
                    ),
                    BridgeSheetBindingOption(
                        fileId: "Wrong.mp3",
                        name: "Wrong.mp3",
                        offer: .refusedCodec(codec: "MP3")
                    ),
                    BridgeSheetBindingOption(
                        fileId: "Short.flac",
                        name: "Short.flac",
                        offer: .refusedTiming
                    ),
                ]
            ),
            BridgeSheetReferenceOptions(
                fileReference: "Second.wav",
                bound: secondAssigned
                    ? BridgeMappingContainer(
                        fileId: "Second.flac",
                        name: "Second.flac"
                    ) : nil,
                options: [
                    BridgeSheetBindingOption(
                        fileId: "Second.flac",
                        name: "Second.flac",
                        offer: .offered
                    )
                ]
            ),
        ]
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
                    bound: BridgeMappingContainer(
                        fileId: "\(name).flac",
                        name: "\(name).flac"
                    ),
                    options: hasOffers
                        ? [
                            BridgeSheetBindingOption(
                                fileId: "\(name).flac",
                                name: "\(name).flac",
                                offer: .offered
                            )
                        ] : []
                )
            }
        let group = BridgeSheetGroup(
            sheetId: "Collection.cue",
            name: "Collection.cue",
            bound: .describesFiles(audioFileCount: 2),
            boundAudio: .several(references: references),
            unboundReferences: nil,
            assigned: .disc(1),
            sideChoices: BridgeSheetSideChoice.discs(1)
        )
        let size = NSSize(width: 900, height: 180)
        try await withHostedSheet(group, size: size) { _, host in
            try await SnapshotTestSupport.settle(host)
            let menus = HostedMenus.controls(in: host)
                .sorted {
                    $0.convert($0.bounds, to: host).minX
                        < $1.convert($1.bounds, to: host).minX
                }
            try #require(
                menus.count == 2,
                "the place pop-up, then the summary"
            )
            #expect(try HostedMenus.opens(menus[1]))
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
            ReleaseSheetCaption(
                sheet: sheet,
                actions: actions(recording: MappingTrackActionRecorder())
                    .sheetCaption
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
