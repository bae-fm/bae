import BaeKit

/// One physical pressing under a release-group card, on every source that
/// lists it. Mirrors `BridgePressing`: core pairs the two sources' releases,
/// orders them MusicBrainz first, and settles what picking the row claims.
struct Pressing: Equatable, Identifiable {
    /// The release whose facts the row shows.
    let lead: BridgeMetadataResult
    /// Every source's record of this pressing, `lead` first — the names the
    /// row's tags carry. The row is picked whole, so these are not separate
    /// picks.
    let releases: [BridgeMetadataResult]
    /// What picking this row claims, as core settled it.
    let provenance: BridgeMetadataProvenance

    /// Row identity is the lead release's id — stable across a re-search of
    /// the same pressing, so SwiftUI keeps the row rather than tearing it down.
    var id: String {
        lead.releaseId
    }

    /// The same claim, in the shape a release already in the library takes.
    var reseed: BridgeReleaseReseed {
        switch provenance {
        case .externalRelease(let source, let releaseId, let partners):
            .externalRelease(
                releaseId: releaseId,
                source: source,
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
