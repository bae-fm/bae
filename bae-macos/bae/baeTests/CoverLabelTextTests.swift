import BaeKit
import Foundation
import Testing

@Suite("Cover labels")
struct CoverLabelTextTests {
    private func labelText(
        _ label: BridgeCoverLabel,
        sourceName: String = "Cover Art Archive"
    ) -> String {
        coverLabelText(sourceName: sourceName, label: label)
    }

    @Test("A lead image is named by its service")
    func leadIsTheService() {
        #expect(
            labelText(.lead(releaseGroup: false)) == "Cover Art Archive"
        )
    }

    @Test("The album's front image says it is the album's")
    func releaseGroupLeadSaysAlbum() {
        #expect(
            labelText(.lead(releaseGroup: true))
                == localizedCoreString(
                    "core.cover.label.release_group",
                    "Cover Art Archive"
                )
        )
    }

    @Test("A gallery entry names its number, what it shows and the comment")
    func galleryEntryHasEveryPart() {
        let text = labelText(
            .gallery(
                reference: nil,
                position: 2,
                types: [.back],
                comment: "liner notes"
            )
        )
        #expect(text == "Cover Art Archive · 2 · Back · liner notes")
    }

    @Test("A Discogs entry names its release and has no types or comment")
    func discogsEntryNamesItsRelease() {
        let text = labelText(
            .gallery(
                reference: "r123",
                position: 1,
                types: [],
                comment: ""
            ),
            sourceName: "Discogs"
        )
        #expect(text == "Discogs · [r123] · 1")
    }
}
