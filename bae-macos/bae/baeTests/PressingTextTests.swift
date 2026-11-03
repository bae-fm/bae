import BaeKit
import Testing

@testable import bae

/// A pressing's lines read the same shape whichever catalog stated it: the
/// place and the media, then what sets it apart.
struct PressingTextTests {
    private var separator: String {
        localizedCoreString("core.audio.list_separator")
    }

    /// A pressing's lines, as core words them onto its record.
    private func summary(_ facts: BridgePressingFacts) -> String {
        PressingText.line(bridgePressingSummary(facts: facts))
    }

    private func details(_ facts: BridgePressingFacts) -> String {
        PressingText.line(bridgePressingDetails(facts: facts))
    }

    @Test("a MusicBrainz and a Discogs record of one pressing read alike")
    func bothCatalogsReadTheSameShape() {
        let musicBrainz = PreviewData.pressingFacts(
            country: "JP",
            media: PreviewData.media(.cd),
            status: .official,
            packaging: .jewelCase
        )
        let discogs = PreviewData.pressingFacts(
            country: "JP",
            media: PreviewData.media(.cd),
            status: .promotion,
            discogsDetails: [.reissue, .flac]
        )

        #expect(
            summary(musicBrainz) == summary(discogs)
        )
        #expect(
            summary(discogs)
                == [PressingText.countryName("JP"), "CD"]
                .joined(separator: separator)
        )
        #expect(
            details(musicBrainz)
                == localizedCoreString("core.pressing.packaging.jewel_case"),
            "an official release is not set apart by being official"
        )
        #expect(
            details(discogs)
                == [
                    localizedCoreString("core.pressing.status.promotion"),
                    localizedCoreString("core.pressing.discogs.reissue"),
                    "FLAC",
                ]
                .joined(separator: separator)
        )
    }

    @Test("more than one of a medium carries its count")
    func countedMedia() {
        let text = PressingText.media(PreviewData.media(.cd, 2))
        #expect(text.contains("2"))
        #expect(text.contains("CD"))
        #expect(PressingText.media([]).isEmpty)
    }

    @Test("a region is named through the catalog")
    func regionName() {
        let facts = PreviewData.pressingFacts(region: .ukAndEurope)
        #expect(
            summary(facts)
                == localizedCoreString("core.pressing.region.uk_and_europe")
        )
        #expect(
            summary(facts) != "core.pressing.region.uk_and_europe"
        )
    }
}
