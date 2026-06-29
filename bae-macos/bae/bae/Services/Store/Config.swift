import Foundation

// MARK: - DiscogsTokenStatus

enum DiscogsTokenStatus: Equatable {
    case notConfigured
    case valid
    /// A key is stored but Discogs hasn't confirmed it yet (saved offline or
    /// rate-limited). Used optimistically; re-checked when possible.
    case unvalidated
    /// Discogs returned 401 for the stored key. Not used until re-saved.
    case rejected

    init(bridge: BridgeDiscogsTokenStatus) {
        switch bridge {
        case .notConfigured: self = .notConfigured
        case .valid: self = .valid
        case .unvalidated: self = .unvalidated
        case .rejected: self = .rejected
        }
    }
}

// MARK: - Config

struct Config: Equatable {
    let libraryId: String
    let libraryName: String
    let libraryPath: String
    let discogsTokenStatus: DiscogsTokenStatus
    /// Whether Discogs can be used as a metadata source. Core decides the policy
    /// (a stored key that isn't rejected); the UI reads this, not the status.
    let discogsUsable: Bool
    /// The configured cloud provider, present whenever YAML carries one.
    /// "Configured" — not "live". Flows that only need a provider configured
    /// (managed import enqueues a cloud_outbox row and uploads on the next
    /// sync cycle) gate on `sync != nil`. Flows that need sync live this
    /// instant (device pairing, restore-code generation) gate on
    /// `ConfigStore.syncReady` — that's runtime status, kept off this
    /// persisted-config mirror.
    let sync: BridgeSyncConfig?
    let pauseBetweenSides: Bool
    let mcp: BridgeMcpConfig

    var hasCloudHome: Bool { sync != nil }

    init(bridge: BridgeConfig) {
        libraryId = bridge.libraryId
        libraryName = bridge.libraryName
        libraryPath = bridge.libraryPath
        discogsTokenStatus = DiscogsTokenStatus(
            bridge: bridge.discogsTokenStatus
        )
        discogsUsable = bridge.discogsUsable
        sync = bridge.sync
        pauseBetweenSides = bridge.pauseBetweenSides
        mcp = bridge.mcp
    }

    /// The storage state to import into. `Managed` only when a cloud home
    /// exists and the user chose it; otherwise `Unmanaged`. Whether to keep the
    /// release pinned (offline) is the orthogonal `pin` argument to
    /// `startImport`, never folded in here.
    func importStorageMode(managed: Bool) -> BridgeStorageMode {
        hasCloudHome && managed ? .remote : .local
    }
}
