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
    /// What picking this row claims, as core settled it.
    let provenance: BridgeMetadataProvenance

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
        switch provenance {
        case .externalRelease(let record, let partners):
            .externalRelease(
                releaseId: record.key,
                source: record.catalog,
                partners: partners
            )
        case .fileTags:
            .fileTags
        }
    }

    /// `nil` for a pressing carrying no releases, which core does not build:
    /// a pressing exists because at least one source listed it.
    init?(bridge: BridgePressing) {
        guard let lead = bridge.releases.first else { return nil }
        self.lead = lead
        releases = bridge.releases
        provenance = bridge.pick
    }
}
