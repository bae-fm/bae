import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The names read off an object are one answer drawn in three places: under
/// the import pane's audio facts, in a candidate row's hover, and behind the
/// library expansion's facts line.
@MainActor
@Suite("Release marks")
struct ReleaseMarksTests {
    private static let paneSize = NSSize(width: 900, height: 1800)
    private static let lineSize = NSSize(width: 460, height: 160)

    /// Each line states what kind of name it is, the value as it was read, and
    /// a tag for every surface it was read from.
    @Test("every mark draws its kind, its value and its surfaces")
    func everyMarkDrawsItsKindValueAndSurfaces() async throws {
        let lines = try await FindOnlineRendering.text(
            MarkLines(marks: PreviewData.releaseMarks)
                .padding()
                .frame(width: Self.lineSize.width)
                .preferredColorScheme(.light)
                .background(.white),
            size: Self.lineSize
        )
        for kind in [
            BridgeMarkKind.discId, .barcode, .catalogNumber,
        ] {
            let label = coreString(bridgeMarkKindKey(kind: kind))
            #expect(lines.carrying(label), "\(label) is missing from \(lines)")
        }
        #expect(lines.carrying("0075678164521"))
        #expect(lines.carrying("7559-60691-2"))
        for origin in [
            BridgeSignalOrigin.discToc, .artwork, .cueSheet, .folderName,
        ] {
            let tag = coreString(bridgeSignalOriginKey(origin: origin))
            #expect(lines.carrying(tag), "\(tag) is missing from \(lines)")
        }
    }

    /// The pane states the folder's names once, under the audio facts.
    @Test("the pane draws one line per name the folder states")
    func thePaneDrawsTheFoldersNames() async throws {
        let lines = try await paneText(marks: PreviewData.releaseMarks)
        #expect(lines.carrying("0075678164521"))
        #expect(lines.carrying("7559-60691-2"))
    }

    /// A candidate nothing was read off draws no lines at all.
    @Test("a candidate nothing was read off draws no mark lines")
    func aCandidateWithoutMarksDrawsNone() async throws {
        let lines = try await paneText(marks: [])
        // The values, not the kind labels: the pressing form has its own
        // Barcode and Catalog fields, and those stay whatever the folder
        // states.
        for value in PreviewData.releaseMarks.map(\.value) {
            #expect(
                !lines.carrying(value),
                "\(value) has no line to state it: \(lines)"
            )
        }
        #expect(
            !lines.carrying(
                coreString(bridgeMarkKindKey(kind: .discId))
            )
        )
    }

    /// The row's hover leads with the folder's names, above the catalogs.
    @Test("the identified hover states the folder's names and its catalogs")
    func theHoverStatesNamesAndCatalogs() async throws {
        let lines = try await FindOnlineRendering.text(
            IdentifiedFromPopover(
                marks: PreviewData.releaseMarks,
                records: PreviewData.releaseRecordsPair
            )
            .preferredColorScheme(.light)
            .background(.white),
            size: NSSize(width: 300, height: 260)
        )
        #expect(lines.carrying("0075678164521"))
        #expect(
            lines.carrying(
                coreString("core.import.triage.identified_from").uppercased()
            )
        )
        #expect(lines.carrying("MusicBrainz"))
    }

    /// The library expansion's popover states the release's own names above
    /// the catalogs that describe it — the two halves of where its facts came
    /// from, in one place.
    @Test("the expansion popover states the release's names and its catalogs")
    func theExpansionPopoverStatesNamesAndCatalogs() async throws {
        let store = PreviewData.seededLibraryStore()
        let summary = try #require(store.albumSummaries["a-01"])
        let release = try #require(store.releaseDetails[summary.releaseIds[0]])
        #expect(!release.marks.isEmpty, "the fixture release carries marks")
        #expect(!release.records.isEmpty, "the fixture release carries records")

        let lines = try await FindOnlineRendering.text(
            ReleaseFactsPopover(
                marks: release.marks,
                records: release.records
            )
            .preferredColorScheme(.light)
            .background(.white),
            size: NSSize(width: 300, height: 220)
        )

        // A barcode fits the popover's width, so the line states it whole.
        let barcode = try #require(
            release.marks.first { $0.kind == .barcode }
        )
        #expect(
            lines.carrying(coreString(bridgeMarkKindKey(kind: barcode.kind)))
        )
        #expect(lines.carrying(barcode.value))
        for origin in barcode.origins {
            let tag = coreString(bridgeSignalOriginKey(origin: origin))
            #expect(lines.carrying(tag), "\(tag) is missing from \(lines)")
        }
        // A disc ID does not fit, and its middle is what gives way: both ends
        // stay, because they are what tells two discs apart.
        let discId = try #require(release.marks.first { $0.kind == .discId })
        #expect(!lines.carrying(discId.value))
        #expect(lines.carrying(String(discId.value.prefix(8))))
        #expect(lines.carrying(String(discId.value.suffix(8))))
        for record in release.records {
            let name = bridgeCatalogName(catalog: record.catalog)
            #expect(lines.carrying(name), "\(name) is missing from \(lines)")
        }
    }

    private func paneText(marks: [BridgeReleaseMark]) async throws -> [String] {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable,
            marks: marks
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
