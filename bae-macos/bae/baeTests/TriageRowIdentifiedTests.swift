import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What a candidate row draws about its release. Which facts a row holds is
/// core's answer and core tests it; these are about the glyphs the row draws
/// once it has them, and about the card either glyph opens.
@Suite("Triage row rendering")
struct TriageRowIdentifiedTests {
    @MainActor
    @Test("an unidentified row draws its folder, with its cover tile kept")
    func unidentifiedRowDrawsItsFolder() async throws {
        let lines = try await renderedLines(PreviewData.triageRowUnidentified)
        #expect(lines.carrying("Release Folder Eleven"))
        #expect(!lines.carrying("Album Title"))

        let unidentified = hostedRow(PreviewData.triageRowUnidentified)
        let prefilled = hostedRow(PreviewData.triageRowPrefilledFromTags)
        #expect(
            unidentified.fittingSize.height >= TriageRowView.coverPointSize
        )
        #expect(
            unidentified.fittingSize.height == prefilled.fittingSize.height
        )
    }

    @MainActor
    @Test("a row filled from tags draws its draft and no glyph")
    func prefilledRowDrawsItsDraft() async throws {
        let lines = try await renderedLines(
            PreviewData.triageRowPrefilledFromTags
        )
        #expect(lines.carrying("Album Title Twelve"))
        #expect(lines.carrying("Artist Name"))
        #expect(!lines.carrying("Release Folder Twelve"))
    }

    /// Neither word is ever drawn: what the glyphs say is said by the glyphs.
    @MainActor
    @Test("a row states its two facts without naming either")
    func aRowNamesNeitherFact() async throws {
        let lines = try await renderedLines(PreviewData.triageRowSealAndCheck)
        #expect(lines.carrying("Album Title Fifteen"))
        #expect(!lines.carrying(coreString("core.identity.identified")))
        #expect(!lines.carrying(coreString("core.identity.verified")))
    }

    /// Verification stays in the detail: toggling it changes no list pixels.
    @MainActor
    @Test("rip verification adds no check to a candidate row")
    func verificationAddsNoListCheck() async throws {
        #expect(
            try await pixels(of: PreviewData.triageRowSealAndCheck)
                == pixels(of: PreviewData.triageRowSealOnly)
        )
        #expect(
            try await pixels(of: PreviewData.triageRowCheckOnly)
                == pixels(of: PreviewData.triageRowNeitherGlyph)
        )
        #expect(
            try await pixels(of: PreviewData.triageRowSealOnly)
                != pixels(of: PreviewData.triageRowNeitherGlyph)
        )
        let row = PreviewData.triageRowCheckBesideMatches
        #expect(try await renderedLines(row).carrying("3 matches"))
        var unverified = row
        unverified.verified = false
        #expect(try await pixels(of: row) == pixels(of: unverified))
    }

    /// On a selected row the whole text column goes white, and the glyphs
    /// follow it rather than keeping their own colour.
    @MainActor
    @Test("a selected row draws its glyphs in the column's colour")
    func aSelectedRowDrawsItsGlyphsInTheColumnsColour() async throws {
        let row = PreviewData.triageRowSealAndCheck
        let resting = try await pixels(of: row)
        let selected = try await pixels(of: row, prominence: .increased)
        #expect(resting != selected)
    }

    /// Every catalog that describes the pressing the pick claimed, plus what
    /// the folder states and what the databases said — the card is all three.
    @MainActor
    @Test("the card names every catalog that describes the release")
    func theCardNamesEveryCatalog() async throws {
        let hosted = SnapshotTestSupport.hostInWindow(
            ReleaseFactsPopover(
                marks: [],
                verification: nil,
                records: PreviewData.identifiedFromBothCatalogs
            )
            .preferredColorScheme(.light)
            .background(.white),
            size: Self.popoverSize
        )
        await SnapshotTestSupport.settle(hosted.host)
        let png = try await SnapshotTestSupport.capturePNG(
            hosted.host,
            size: Self.popoverSize
        )
        let lines =
            try await SnapshotTestSupport.recognizedText(
                in: png,
                languages: ["en-US"]
            )
            .map(\.text)

        #expect(lines.carrying("MusicBrainz"))
        #expect(lines.carrying("Discogs"))
    }

    private static let rowSize = NSSize(width: 340, height: 80)
    private static let popoverSize = NSSize(
        width: ReleaseFactsPopover.width,
        height: 96
    )

    @MainActor
    private func hostedRow(
        _ row: BridgeTriageRow,
        prominence: BackgroundProminence = .standard
    ) -> NSView {
        SnapshotTestSupport.hostInWindow(
            TriageRowView(
                row: row,
                coverContent: nil,
                uploadObservation: nil,
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            .environment(\.backgroundProminence, prominence)
            .environment(ImageStore.stub())
            .preferredColorScheme(.light)
            .background(.white)
            .frame(width: Self.rowSize.width),
            size: Self.rowSize
        )
        .host
    }

    @MainActor
    private func pixels(
        of row: BridgeTriageRow,
        prominence: BackgroundProminence = .standard
    ) async throws -> Data {
        let host = hostedRow(row, prominence: prominence)
        await SnapshotTestSupport.settle(host)
        return try await SnapshotTestSupport.capturePNG(
            host,
            size: Self.rowSize
        )
    }

    @MainActor
    private func renderedLines(_ row: BridgeTriageRow) async throws -> [String]
    {
        try await SnapshotTestSupport.recognizedText(
            in: pixels(of: row),
            languages: ["en-US"]
        )
        .map(\.text)
    }
}
