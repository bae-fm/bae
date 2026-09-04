import BaeKit

/// One physical pressing under a release-group card, on every source that
/// lists it. Mirrors `BridgePressing`: core pairs the two sources' releases and
/// orders them, MusicBrainz first. A row is picked whole — `lead` is what the
/// draft is read from and the rest ride along as partners.
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

    /// Every source's record of this pressing other than the lead's — what a
    /// pick carries alongside the release its draft is read from.
    var partners: [BridgeMetadataRef] {
        releases.dropFirst()
            .map { release in
                BridgeMetadataRef(
                    source: release.source,
                    releaseId: release.releaseId
                )
            }
    }

    /// What picking this row claims for an import candidate: the lead is the
    /// release the draft is read from, and the partners ride along. One place,
    /// so a row pick means the same thing wherever it is made.
    var provenance: BridgeMetadataProvenance {
        .externalRelease(
            source: lead.source,
            releaseId: lead.releaseId,
            partners: partners
        )
    }

    /// The same claim for a release already in the library.
    var reseed: BridgeReleaseReseed {
        .externalRelease(
            releaseId: lead.releaseId,
            source: lead.source,
            partners: partners
        )
    }

    /// `nil` for a pressing carrying no releases, which core does not build:
    /// a pressing exists because at least one source listed it.
    init?(bridge: BridgePressing) {
        guard let lead = bridge.releases.first else { return nil }
        self.lead = lead
        releases = bridge.releases
    }
}
