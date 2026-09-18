import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What the dot after a release field says, and when it says nothing.
@MainActor
@Suite("Field origin dots")
struct FieldOriginDotTests {
    /// What one field's dot draws, at a size that holds nothing else.
    private func dotPixels(
        _ provenance: BridgeFieldProvenance?
    ) async throws -> Data {
        let size = NSSize(width: 16, height: 16)
        return try await FindOnlineRendering.pixels(
            Group {
                if let provenance {
                    FieldOriginDot(provenance: provenance)
                }
            }
            .frame(width: size.width, height: size.height),
            size: size
        )
    }

    /// A field core marked draws a dot, and a field it marked nothing for
    /// draws exactly what no dot at all draws. Which two of the eight are
    /// marked is core's answer — a label the person typed, and a catalog
    /// number the catalogs state differently.
    @Test("a dot is drawn for the marked fields and for no others")
    func aDotIsDrawnForTheMarkedFieldsAndForNoOthers() async throws {
        let blank = try await dotPixels(nil)
        var marked: Set<BridgeCandidateEditField> = []
        for entry in PreviewData.fieldProvenance()
        where try await dotPixels(entry) != blank {
            marked.insert(entry.field)
        }
        #expect(marked == [.label, .catalogNumber])
    }

    /// The grid puts those dots in the form: drawing it with what core says
    /// differs from drawing it with nothing said, and a form core marked no
    /// field of draws the same as one it described not at all.
    @Test("the grid draws the dots and only when core marks one")
    func theGridDrawsTheDotsAndOnlyWhenCoreMarksOne() async throws {
        func grid(_ provenance: [BridgeFieldProvenance]) async throws -> Data {
            try await FindOnlineRendering.pixels(
                ReleasePressingFieldsGrid(
                    values: PreviewData.editMetadataDraft(trackCount: 1),
                    provenance: provenance,
                    writer: ReleaseFieldWriter { _, _ in },
                    editingCommands: EditingCommitCommands()
                )
                .padding(16)
                .environment(Library.stub())
                .environment(UiStore()),
                size: NSSize(width: 520, height: 220)
            )
        }
        let undescribed = try await grid([])
        let unmarked = try await grid(
            PreviewData.fieldProvenance()
                .map {
                    BridgeFieldProvenance(
                        field: $0.field,
                        origin: $0.origin,
                        claims: $0.claims,
                        dot: nil
                    )
                }
        )
        #expect(
            unmarked == undescribed,
            "a form core marked no field of draws no dots"
        )
        #expect(
            try await grid(PreviewData.fieldProvenance()) != undescribed,
            "the fields core marked draw theirs"
        )
    }

    /// A field a person types in is theirs before the form is ever saved, and
    /// emptying it leaves no value for an origin to describe. Core's rule,
    /// asked rather than re-derived, so the sheet's dot matches the one the
    /// stored release draws.
    @Test("typing marks the field and emptying it unmarks it")
    func typingMarksTheFieldAndEmptyingItUnmarksIt() async throws {
        let seed = PreviewData.releaseEditSeed(trackCount: 1)
        let session = ReleaseMetadataEditSession(
            releaseId: "release-1",
            seed: seed,
            save: { _, _ in },
            reset: { _ in
                BridgeReleaseFormReset(
                    edit: seed.edit,
                    fieldProvenance: seed.fieldProvenance
                )
            }
        )

        await session.fieldWriter.setField(.country, "JP")
        #expect(session.fieldProvenance.forField(.country)?.dot == .typed)
        #expect(session.form.origins.country == .typed)

        await session.fieldWriter.setField(.country, "")
        #expect(session.fieldProvenance.forField(.country)?.dot == nil)
        #expect(session.form.origins.country == nil)
    }

    /// Typing over a field the catalogs disagree about leaves the dot saying
    /// so: the readings behind it are still what it invites a look at.
    @Test("a disagreement outlives the value being typed over")
    func aDisagreementOutlivesTheValueBeingTypedOver() async throws {
        let seed = PreviewData.releaseEditSeed(trackCount: 1)
        let session = ReleaseMetadataEditSession(
            releaseId: "release-1",
            seed: seed,
            save: { _, _ in },
            reset: { _ in
                BridgeReleaseFormReset(
                    edit: seed.edit,
                    fieldProvenance: seed.fieldProvenance
                )
            }
        )

        await session.fieldWriter.setField(.catalogNumber, "CAT-0002")
        let entry = try #require(
            session.fieldProvenance.forField(.catalogNumber)
        )
        #expect(entry.origin == .typed)
        #expect(entry.dot == .disagreement)
    }

    /// The hover behind a dot names every catalog describing the release and
    /// what each one states, plus where the value in the field came from when
    /// it was not a catalog.
    @Test("the hover lists both catalogs' readings")
    func theHoverListsBothCatalogsReadings() async throws {
        let lines = try await FindOnlineRendering.text(
            FieldOriginPopover(
                provenance: PreviewData.disagreeingCatalogNumber
            ),
            size: NSSize(width: 260, height: 90)
        )
        for expected in ["MusicBrainz", "Discogs", "CAT-0001", "CAT-0001-A"] {
            #expect(
                lines.contains {
                    $0.localizedCaseInsensitiveContains(expected)
                },
                "the hover read: \(lines)"
            )
        }
    }

    /// A value a person typed says so under the readings, so the hover
    /// explains the grey dot rather than leaving it to be guessed at.
    @Test("a typed value says so in its hover")
    func aTypedValueSaysSoInItsHover() async throws {
        let lines = try await FindOnlineRendering.text(
            FieldOriginPopover(provenance: PreviewData.typedLabel),
            size: NSSize(width: 260, height: 110)
        )
        let typed = coreString("core.field.origin.typed")
        #expect(
            lines.contains { $0.localizedCaseInsensitiveContains(typed) },
            "the hover read: \(lines)"
        )
    }
}
