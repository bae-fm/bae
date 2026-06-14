import Foundation

/// User's identity claim from the import flow.
///
/// - `exact`: "this IS my pressing." The committed `release_identities`
///   row carries `source_release_id` set to the picked release.
///   Pressing metadata seeds from the picked release. The associated
///   `releaseId` / `source` identify the picked release the worker
///   fetches at commit time.
/// - `approximate`: "this is my album, but I don't claim a specific
///   pressing." The committed `release_identities` row carries
///   `source_release_id = NULL`. Pressing metadata stays NULL — only
///   album-group-stable fields (title, artist, tracks) seed.
/// - `unknown`: no identity claim. Zero `release_identities` rows are
///   written; metadata seeds from the rip's embedded file tags and the
///   release lands on a fresh album.
///
/// Mirrors `bae_core::import::IdentityChoice` via `BridgeIdentityChoice`.
enum IdentityChoice: Equatable {
    case exact(releaseId: String, source: MetadataSource)
    case approximate(releaseId: String, source: MetadataSource)
    case unknown

    var bridge: BridgeIdentityChoice {
        switch self {
        case .exact(let releaseId, let source):
            .exact(releaseId: releaseId, source: source.bridge)
        case .approximate(let releaseId, let source):
            .approximate(releaseId: releaseId, source: source.bridge)
        case .unknown:
            .unknown
        }
    }
}
