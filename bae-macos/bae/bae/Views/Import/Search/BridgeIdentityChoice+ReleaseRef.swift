import BaeKit

extension BridgeIdentityChoice {
    /// The picked release this choice points at. Exact and Approximate both
    /// name a release (Approximate just NULLs `source_release_id` at commit);
    /// Unknown carries no reference. Lets the pane's Import-as toggle rebuild
    /// the other variant against the same release.
    var releaseRef: (releaseId: String, source: BridgeMetadataSource)? {
        switch self {
        case .exact(let releaseId, let source),
            .approximate(let releaseId, let source):
            (releaseId, source)
        case .unknown:
            nil
        }
    }

    /// Whether this is the Metadata-only (Approximate) claim.
    var isApproximate: Bool {
        if case .approximate = self {
            return true
        }
        return false
    }

    /// Build Exact or Metadata-only (Approximate) against the same picked
    /// release — the choice the pane's Import-as toggle and the re-identify
    /// footer both flip.
    static func make(
        exact: Bool,
        releaseId: String,
        source: BridgeMetadataSource
    )
        -> BridgeIdentityChoice
    {
        exact
            ? .exact(releaseId: releaseId, source: source)
            : .approximate(releaseId: releaseId, source: source)
    }
}
