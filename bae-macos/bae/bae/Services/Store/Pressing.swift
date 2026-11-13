import BaeKit

/// One physical pressing under a release-group card, on every source that
/// lists it. Mirrors `BridgePressing`: core pairs the two sources' releases,
/// orders them by what the folder says about each, and settles what picking
/// the row claims.
struct Pressing: Equatable, Identifiable {
    /// The release whose facts the row shows.
    let lead: BridgeMetadataResult
    /// Every source's record of this pressing, `lead` first. The row is picked
    /// whole, so these are not separate picks.
    let releases: [BridgeMetadataResult]
    /// Every label the records state, each name once with its catalog
    /// numbers, in the order of the record the row leads with.
    let labels: [BridgeStatedLabelLine]
    /// The years the records state: each record's where they differ, each
    /// followed by its catalogs.
    let years: [StatedValue]
    /// The barcodes the records state, listed as `years` are.
    let barcodes: [StatedValue]
    /// Where the records say it was released, listed as `years` are.
    let areas: [StatedValue]
    /// What the records say it is made of, listed as `years` are.
    let media: [StatedValue]
    /// Why identification could not read a record's full document, which the
    /// row then shows as its search result stated it.
    let documentFailure: BridgeLookupFailure?
    /// What picking this row claims, as core settled it.
    let link: BridgePressingLink
    /// Every source listing this pressing, in core's order — the names the
    /// row's tags carry, whichever record the row is read from.
    let sources: [BridgeLookupCatalog]
    /// What sets it apart beyond that, as its lead record states it:
    /// "Promo · Reissue".
    let detailsText: String

    /// Where it was released and what it is made of, as one line of text:
    /// "2×CD · Japan".
    var summaryText: String {
        FactList.joined([
            PressingText.alternatives(media),
            PressingText.alternatives(areas),
        ])
    }

    /// Row identity is the lead release's id — stable across a re-search of
    /// the same pressing, so SwiftUI keeps the row rather than tearing it down.
    var id: String {
        lead.releaseId
    }

    /// `nil` for a pressing carrying no releases, which core does not build:
    /// a pressing exists because at least one source listed it.
    init?(bridge: BridgePressing) {
        guard let lead = bridge.releases.first else { return nil }
        self.lead = lead
        releases = bridge.releases
        labels = bridge.labels
        years = bridge.years.map(\.stated)
        barcodes = bridge.barcodes.map(\.stated)
        areas = bridge.areas.map(\.stated)
        media = bridge.media.map(\.stated)
        documentFailure = bridge.documentFailure
        link = bridge.pick
        sources = bridge.sources
        detailsText = PressingText.line(bridge.details)
    }
}
