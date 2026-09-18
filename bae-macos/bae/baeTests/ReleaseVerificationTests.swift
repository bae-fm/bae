import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What the rip databases said is one line drawn under the names the object
/// states: in the import pane, in a candidate row's hover, and behind the
/// library expansion's facts line.
@MainActor
@Suite("Release verification")
struct ReleaseVerificationTests {
    private static let paneSize = NSSize(width: 900, height: 1800)
    private static let lineSize = NSSize(width: 460, height: 120)

    /// The line states the count in the words the locale renders it with, so
    /// the surface never composes the sentence itself.
    @Test("the line states how many other rips match")
    func theLineStatesTheCount() async throws {
        let lines = try await FindOnlineRendering.text(
            RipMatchLine(verification: PreviewData.releaseVerification)
                .padding()
                .frame(width: Self.lineSize.width)
                .preferredColorScheme(.light)
                .background(.white),
            size: Self.lineSize,
            scale: 3
        )
        #expect(
            lines.carrying(coreString(bridgeSignalOriginKey(origin: .discToc)))
        )
        #expect(
            lines.carrying(
                coreString("core.verification.matches_other_rips", 37)
            ),
            "the rendered line is missing from \(lines)"
        )
    }

    /// A release whose every track no database confirmed has no count, and a
    /// line that states nothing is no line.
    @Test("a rip nothing confirmed draws no line")
    func anUnconfirmedRipDrawsNoLine() async throws {
        let lines = try await FindOnlineRendering.text(
            RipMatchLine(
                verification: BridgeVerification(
                    source: .log,
                    matchedCopies: nil,
                    tracks: []
                )
            )
            .padding()
            .frame(width: Self.lineSize.width)
            .preferredColorScheme(.light)
            .background(.white),
            size: Self.lineSize
        )
        #expect(lines.isEmpty, "nothing is drawn, got \(lines)")
    }

    /// The pane states it once, under the names the folder carries.
    @Test("the pane draws the rip-match line under the folder's names")
    func thePaneDrawsTheLine() async throws {
        let matched = try await paneText(
            verification: PreviewData.releaseVerification
        )
        #expect(
            matched.carrying(
                coreString("core.verification.matches_other_rips", 37)
            ),
            "the pane is missing the line: \(matched)"
        )

        let unread = try await paneText(verification: nil)
        #expect(
            !unread.carrying(
                coreString("core.verification.matches_other_rips", 37)
            ),
            "a folder whose log says nothing has no line: \(unread)"
        )
    }

    /// The glyphs' card states it with the names, above the catalogs.
    @Test("the glyph card states what the databases said")
    func theCardStatesTheCount() async throws {
        let lines = try await FindOnlineRendering.text(
            ReleaseFactsPopover(
                marks: PreviewData.releaseMarks,
                verification: PreviewData.releaseVerification,
                records: PreviewData.releaseRecordsPair
            )
            .preferredColorScheme(.light)
            .background(.white),
            size: NSSize(width: ReleaseFactsPopover.width, height: 300)
        )
        #expect(
            lines.carrying(
                coreString("core.verification.matches_other_rips", 37)
            ),
            "the hover is missing the line: \(lines)"
        )
    }

    /// The library expansion's popover states it with the release's own names
    /// and the catalogs that describe it.
    @Test("the expansion popover states what the databases said")
    func theExpansionPopoverStatesTheCount() async throws {
        let store = PreviewData.seededLibraryStore()
        let summary = try #require(store.albumSummaries["a-01"])
        let release = try #require(store.releaseDetails[summary.releaseIds[0]])
        let verification = try #require(release.verification)

        let lines = try await FindOnlineRendering.text(
            ReleaseFactsPopover(
                marks: release.marks,
                verification: verification,
                records: release.records
            )
            .preferredColorScheme(.light)
            .background(.white),
            size: NSSize(width: ReleaseFactsPopover.width, height: 260)
        )
        let count = try #require(verification.matchedCopies)
        #expect(
            lines.carrying(
                coreString(
                    "core.verification.matches_other_rips",
                    Int(count)
                )
            ),
            "the popover is missing the line: \(lines)"
        )
    }

    private func paneText(
        verification: BridgeVerification?
    ) async throws -> [String] {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable,
            verification: verification
        )
        let candidate = try #require(
            store.selectedCandidates[MappingFixtures.candidateKey]
        )
        return try await FindOnlineRendering.text(
            ImportMappingPreview.make(
                candidate: candidate,
                storageCloud: .constant(false),
                storagePinned: .constant(false)
            )
            .environment(store)
            .importPreviewEnvironment()
            .preferredColorScheme(.light),
            size: Self.paneSize
        )
    }
}
