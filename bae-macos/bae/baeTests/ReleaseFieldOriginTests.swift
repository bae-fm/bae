import BaeKit
import Testing

@testable import bae

@MainActor
struct ReleaseFieldOriginTests {
    @Test("typing records a manual origin and emptying removes it")
    func typingAndClearingOrigins() async throws {
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
        #expect(session.form.origins.country == .typed)

        await session.fieldWriter.setField(.country, "")
        #expect(session.form.origins.country == nil)
    }

    @Test("typing over a catalog value records a manual origin")
    func overridingCatalogOrigin() async throws {
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
        #expect(session.form.origins.catalogNumber == .typed)
    }
}
