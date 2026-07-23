import Foundation

// MARK: - Config

public struct Config: Equatable {
    public let libraryId: String
    public let libraryName: String
    public let libraryPath: String
    public let discogsTokenStatus: BridgeDiscogsTokenStatus
    /// Whether Discogs can be used as a metadata source. Core decides the policy
    /// (a stored key that isn't rejected); the UI reads this, not the status.
    public let discogsUsable: Bool
    /// The configured cloud provider, present whenever YAML carries one.
    /// "Configured" — not "live". Flows that only need a provider configured
    /// (managed import enqueues a cloud_outbox row and uploads on the next
    /// sync cycle) gate on `sync != nil`. Flows that need sync live this
    /// instant (device pairing, restore-code generation) gate on
    /// `ConfigStore.syncReady` — that's runtime status, kept off this
    /// persisted-config mirror.
    public let sync: BridgeSyncConfig?
    public let pauseBetweenSides: Bool
    /// How many blob uploads run at once, and how many downloads a pin fetches at
    /// once. Device-local (a per-machine choice), range 1...8. The storage manager
    /// reads them here and writes through the bridge setters.
    public let maxConcurrentUploads: UInt32
    public let maxConcurrentDownloads: UInt32
    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. A synced preference, so the bar
    /// reads it here rather than keeping its own copy per device.
    public let showRemainingTime: Bool
    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. A synced preference,
    /// read here by the library page.
    public let libraryFullWidth: Bool
    /// Configured export presets offered by release and track export.
    public let savePresets: [BridgeSavePreset]
    /// Id of the preset a track save defaults to (valid + track-applicable).
    public let defaultTrackSavePreset: String
    /// Id of the preset a release save defaults to (valid + release-applicable).
    public let defaultReleaseSavePreset: String
    public let mcp: BridgeMcpConfig
    public let subsonic: BridgeSubsonicConfig

    public var hasCloudHome: Bool { sync != nil }

    public init(bridge: BridgeConfig) {
        libraryId = bridge.libraryId
        libraryName = bridge.libraryName
        libraryPath = bridge.libraryPath
        discogsTokenStatus = bridge.discogsTokenStatus
        discogsUsable = bridge.discogsUsable
        sync = bridge.sync
        pauseBetweenSides = bridge.pauseBetweenSides
        maxConcurrentUploads = bridge.maxConcurrentUploads
        maxConcurrentDownloads = bridge.maxConcurrentDownloads
        showRemainingTime = bridge.showRemainingTime
        libraryFullWidth = bridge.libraryFullWidth
        savePresets = bridge.savePresets
        defaultTrackSavePreset = bridge.defaultTrackSavePreset
        defaultReleaseSavePreset = bridge.defaultReleaseSavePreset
        mcp = bridge.mcp
        subsonic = bridge.subsonic
    }

    /// The storage state to import into. `Managed` only when a cloud home
    /// exists and the user chose it; otherwise `Unmanaged`. Whether to keep the
    /// release pinned (offline) is the orthogonal `pin` argument to
    /// `startImport`, never folded in here.
    public func importStorageMode(managed: Bool) -> BridgeStorageMode {
        hasCloudHome && managed ? .remote : .local
    }
}
