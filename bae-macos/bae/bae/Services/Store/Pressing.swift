import BaeKit

/// One physical pressing under a release-group card, on every source that
/// lists it. Mirrors `BridgePressing`: core pairs the two sources' releases and
/// orders them, MusicBrainz first, so picking the row commits `lead` and a
/// source tag commits that source's own release.
struct Pressing: Equatable, Identifiable {
    /// The release picking the row itself commits.
    let lead: BridgeMetadataResult
    /// Every source's record of this pressing, `lead` first.
    let releases: [BridgeMetadataResult]

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
    }
}
