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
    /// A fixture's pressing row, stating what its lead record states —
    /// what core's own rows state where the other records add nothing.
    init(
        releases: [BridgeMetadataResult],
        sources: [BridgeLookupCatalog],
        labels: [BridgeLabelLine] = [],
        documentFailure: BridgeLookupFailure? = nil,
        pick: BridgePressingLink
    ) {
        let facts = releases.first?.facts ?? PreviewData.pressingFacts()
        self.init(
            releases: releases,
            sources: sources,
            labels: labels,
            documentFailure: documentFailure,
            pick: pick,
            summary: bridgePressingSummary(facts: facts),
            details: bridgePressingDetails(facts: facts)
        )
    }
}
