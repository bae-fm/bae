import BaeKit
import Foundation
import Testing

@Suite("Cover labels")
struct CoverLabelTextTests {
    private func cover(
        _ label: BridgeCoverLabel,
        sourceName: String = "Cover Art Archive"
    ) -> BridgeRemoteCover {
        let image = BridgeRemoteImageSet(
            url: "https://images.example/front.jpg",
            downscaled: []
        )
        return BridgeRemoteCover(
            coverChoice: BridgeCoverChoice(
                selection: .remoteCover(
                    selection: BridgeRemoteCoverSelection(
                        image: image,
                        source: .musicBrainz
                    )
                ),
                image: .remote(image: image)
            ),
            sourceName: sourceName,
            label: label
        )
    }

    @Test("A lead image is named by its service")
    func leadIsTheService() {
        #expect(
            cover(.lead(releaseGroup: false)).labelText == "Cover Art Archive"
        )
    }

    @Test("The album's front image says it is the album's")
    func releaseGroupLeadSaysAlbum() {
        #expect(
            cover(.lead(releaseGroup: true)).labelText
                == localizedCoreString(
                    "core.cover.label.release_group",
                    "Cover Art Archive"
                )
        )
    }

    @Test("A gallery entry names its number, what it shows and the comment")
    func galleryEntryHasEveryPart() {
        let text = cover(
            .gallery(
                reference: nil,
                position: 2,
                types: [.back],
                comment: "liner notes"
            )
        )
        .labelText
        #expect(text == "Cover Art Archive · 2 · Back · liner notes")
    }

    @Test("A Discogs entry names its release and has no types or comment")
    func discogsEntryNamesItsRelease() {
        let text = cover(
            .gallery(
                reference: "r123",
                position: 1,
                types: [],
                comment: ""
            ),
            sourceName: "Discogs"
        )
        .labelText
        #expect(text == "Discogs · [r123] · 1")
    }
}
