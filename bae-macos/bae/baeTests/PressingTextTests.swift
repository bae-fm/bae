import BaeKit
import Testing

@testable import bae

/// A pressing's lines read the same shape whichever catalog stated it: the
/// place and the media, then what sets it apart.
struct PressingTextTests {
    private var separator: String {
        QueueSummary.message("core.audio.list_separator")
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
            PressingText.summary(musicBrainz) == PressingText.summary(discogs)
        )
        #expect(
            PressingText.summary(discogs)
                == [PressingText.countryName("JP"), "CD"]
                .joined(separator: separator)
        )
        #expect(
            PressingText.details(musicBrainz)
                == QueueSummary.message("core.pressing.packaging.jewel_case"),
            "an official release is not set apart by being official"
        )
        #expect(
            PressingText.details(discogs)
                == [
                    QueueSummary.message("core.pressing.status.promotion"),
                    QueueSummary.message("core.pressing.discogs.reissue"),
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
            PressingText.summary(facts)
                == QueueSummary.message("core.pressing.region.uk_and_europe")
        )
        #expect(
            PressingText.summary(facts) != "core.pressing.region.uk_and_europe"
        )
    }
}
