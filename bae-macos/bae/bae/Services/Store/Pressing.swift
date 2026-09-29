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
    let labels: [BridgeLabelLine]
    /// Why identification could not read a record's full document, which the
    /// row then shows as its search result stated it.
    let documentFailure: BridgeLookupFailure?
    /// What picking this row claims, as core settled it.
    let link: BridgeReleaseLink
    /// Where it was released and what it is made of: "Japan · 2×CD".
    let summaryText: String
    /// What sets it apart beyond that: "Promo · Reissue".
    let detailsText: String

    /// Row identity is the lead release's id — stable across a re-search of
    /// the same pressing, so SwiftUI keeps the row rather than tearing it down.
    var id: String {
        lead.releaseId
    }

    /// Every source listing this pressing, in the one order surfaces name
    /// sources in — the names the row's tags carry. Which record the row is
    /// read from decides what the row shows, never what order its names read
    /// in, so a row and the card above it say the same thing.
    var sources: [BridgeCatalog] {
        bridgeLookupCatalogs()
            .filter { source in
                releases.contains { $0.source == source }
            }
    }

    /// The same claim, in the shape a release already in the library takes.
    var reseed: BridgeReleaseReseed {
        .externalRelease(
            releaseId: link.record.key,
            source: link.record.catalog,
            partners: link.partners
        )
    }

    /// `nil` for a pressing carrying no releases, which core does not build:
    /// a pressing exists because at least one source listed it.
    init?(bridge: BridgePressing) {
        guard let lead = bridge.releases.first else { return nil }
        self.lead = lead
        releases = bridge.releases
        labels = bridge.labels
        documentFailure = bridge.documentFailure
        link = bridge.pick
        summaryText = PressingText.line(bridge.summary)
        detailsText = PressingText.line(bridge.details)
    }
}
