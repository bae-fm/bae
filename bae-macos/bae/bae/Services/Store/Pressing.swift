import BaeKit

/// One physical pressing under a release-group card, on every source that
/// lists it. Mirrors `BridgePressing`: core pairs the two sources' releases,
/// orders them by what the folder says about each, and settles what picking
/// the row claims.
struct Pressing: Equatable, Identifiable {
    /// The release whose facts the row shows.
    let lead: BridgeMetadataResult
    /// Every label the records state, each name once with its catalog
    /// numbers, in the order of the record the row leads with.
    let labels: [BridgeStatedLabelLine]
    /// The years the records state: each record's where they differ, each
    /// followed by its catalogs.
    let years: [StatedValue]
    /// Where the records say it was released, listed as `years` are.
    let areas: [StatedValue]
    /// What the records say it is made of, listed as `years` are.
    let media: [StatedValue]
    /// Why identification could not read a record's full document, which the
    /// row then shows as its search result stated it.
    let documentFailure: BridgeLookupFailure?
    /// What picking this row claims, as core settled it.
    let link: BridgePressingLink
    /// What sets it apart beyond that, as its lead record states it, each
    /// detail worded alone: "Promo", "Reissue".
    let details: [String]
    /// Whether the row holds the release core linked the candidate to, or,
    /// before core links it, the release that identification picked on its
    /// own while core reads that release. The list draws this row selected.
    let chosen: Bool
    /// Whether the row holds the release that identification picked on its
    /// own, which core reads in full before it links the candidate to it.
    let pickBeingRead: Bool
    /// Whether the library holds the lead release, as core's statement read
    /// it: the row shows dimmed.
    let inLibrary: Bool

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
        labels = bridge.labels
        years = bridge.years.map(\.stated)
        areas = bridge.areas.map(\.stated)
        media = bridge.media.map(\.stated)
        documentFailure = bridge.documentFailure
        link = bridge.pick
        details = bridge.details.map(\.text)
        chosen = bridge.chosen
        pickBeingRead = bridge.pickBeingRead
        inLibrary = bridge.inLibrary
    }
}
