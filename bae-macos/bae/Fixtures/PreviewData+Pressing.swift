import BaeKit

@testable import bae

extension PreviewData {
    /// What a fixture's pressing is: a country by its ISO code or a
    /// region, and its media. Every other fact is unstated unless a
    /// fixture names it.
    static func pressingFacts(
        country: String? = nil,
        region: BridgeRegion? = nil,
        media: [BridgeMediaCount] = [],
        status: BridgeReleaseStatus? = nil,
        packaging: BridgePackaging? = nil,
        discogsDetails: [BridgeDiscogsDetail] = []
    ) -> BridgePressingFacts {
        BridgePressingFacts(
            area: country.map { .country(code: $0) }
                ?? region.map { .region(region: $0) },
            media: media,
            status: status,
            packaging: packaging,
            discogsDetails: discogsDetails
        )
    }

    /// `count` of one medium.
    static func media(_ medium: BridgeMedium, _ count: UInt32 = 1)
        -> [BridgeMediaCount]
    {
        [BridgeMediaCount(medium: medium, count: count)]
    }
}
extension BridgePressing {
    /// A fixture's pressing row, stating what its lead record states: its
    /// records agree, so no value names a catalog.
    init(
        releases: [BridgeMetadataResult],
        sources: [BridgeLookupCatalog],
        labels: [BridgeLabelLine] = [],
        documentFailure: BridgeLookupFailure? = nil,
        pick: BridgePressingLink
    ) {
        let lead = releases.first
        let facts = lead?.facts ?? PreviewData.pressingFacts()
        let area = bridgePressingSummary(
            facts: BridgePressingFacts(
                area: facts.area,
                media: [],
                status: nil,
                packaging: nil,
                discogsDetails: []
            )
        )
        let media = bridgeMediaTerms(media: facts.media)
        self.init(
            releases: releases,
            sources: sources,
            labels: labels.map { line in
                BridgeStatedLabelLine(
                    names: line.names,
                    catalogNumbers: line.catalogNumbers.map {
                        BridgeTextStatement(text: $0, catalogs: [])
                    }
                )
            },
            years: (lead?.year)
                .map {
                    [BridgeYearStatement(year: $0, catalogs: [])]
                } ?? [],
            barcodes: (lead?.barcodes ?? [])
                .map {
                    BridgeTextStatement(text: $0, catalogs: [])
                },
            documentFailure: documentFailure,
            pick: pick,
            areas: area.isEmpty
                ? [] : [BridgeTermsStatement(terms: area, catalogs: [])],
            media: media.isEmpty
                ? [] : [BridgeTermsStatement(terms: media, catalogs: [])],
            details: bridgePressingDetails(facts: facts)
        )
    }
}
