import BaeKit
import Testing

@testable import bae

/// Where a pressing's records state a fact differently, the row lists every
/// record's value, each followed by the catalogs that state it.
@Suite("Stated values")
struct StatedValueTests {
    private var separator: String {
        localizedCoreString("core.pressing.alternatives_separator")
    }

    @Test("differing years each name their catalog")
    func differingYearsNameTheirCatalogs() {
        let years = [
            BridgeYearStatement(year: 1992, catalogs: [.musicBrainz]),
            BridgeYearStatement(year: 1993, catalogs: [.discogs]),
        ]
        .map(\.stated)

        #expect(
            PressingText.alternatives(years)
                == "1992 \(bridgeLookupCatalogName(catalog: .musicBrainz))"
                + separator
                + "1993 \(bridgeLookupCatalogName(catalog: .discogs))"
        )
    }

    @Test("a value the records agree on names no catalog")
    func agreedValueNamesNoCatalog() {
        let barcode = BridgeTextStatement(
            text: "0 41234 56789 2",
            catalogs: []
        )
        .stated

        #expect(barcode.text == "0 41234 56789 2")
        #expect(PressingText.alternatives([barcode]) == "0 41234 56789 2")
    }

    @Test("a country and a format are worded, each with its catalogs")
    func wordedValuesNameTheirCatalogs() {
        let area = BridgeTermsStatement(
            terms: [.country(code: "US")],
            catalogs: [.musicBrainz]
        )
        .stated

        #expect(
            area.text
                == "\(PressingText.countryName("US")) "
                + bridgeLookupCatalogName(catalog: .musicBrainz)
        )
    }

    @Test("an album's titles are every title, in one line")
    func titlesListEveryTitle() {
        #expect(
            PressingText.alternatives([
                "Low Tide Hours", "Low Tide Hours (Remastered)",
            ])
                == "Low Tide Hours" + separator + "Low Tide Hours (Remastered)"
        )
    }
}
