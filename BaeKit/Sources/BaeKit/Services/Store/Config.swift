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
    /// (cloud import enqueues a cloud_outbox row and uploads on the next
    /// sync cycle) gate on `sync != nil`. Flows that need sync live this
    /// instant (device pairing, restore-code generation) gate on
    /// `SyncStatusStore.syncReady` — that's runtime status, kept off this
    /// persisted-config mirror.
    public let sync: BridgeSyncConfig?
    public let pauseBetweenSides: Bool
    /// How many blob uploads run at once, and how many downloads a pin fetches at
    /// once. Device-local (a per-machine choice), range 1...8. The storage manager
    /// reads them here and writes through the bridge setters.
    public let maxConcurrentUploads: UInt32
    public let maxConcurrentDownloads: UInt32
    /// Whether identification starts on its own: newly discovered candidates are
    /// identified as they are found, and opening Find online starts a run.
    public let identifyAutomatically: Bool
    /// Which source a newly discovered candidate starts with.
    public let defaultImportMetadataSource: BridgeDefaultImportMetadataSource
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
    /// Whether casting to a network receiver is available. Core enforces it —
    /// while off it runs no discovery and refuses to start a session — so the
    /// playback bar reads this only to decide whether to show its Cast control.
    public let castEnabled: Bool
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
        identifyAutomatically = bridge.identifyAutomatically
        defaultImportMetadataSource = bridge.defaultImportMetadataSource
        showRemainingTime = bridge.showRemainingTime
        libraryFullWidth = bridge.libraryFullWidth
        savePresets = bridge.savePresets
        defaultTrackSavePreset = bridge.defaultTrackSavePreset
        defaultReleaseSavePreset = bridge.defaultReleaseSavePreset
        castEnabled = bridge.castEnabled
        mcp = bridge.mcp
        subsonic = bridge.subsonic
    }

    #if os(macOS)
        /// The storage state to import into. `Cloud` only when a cloud home exists
        /// and the user chose it; otherwise `Local`. Whether to keep the
        /// release pinned locally is the orthogonal `pin` argument to
        /// `startImport`, never folded in here.
        public func importStorageMode(cloud: Bool) -> BridgeStorageMode {
            hasCloudHome && cloud ? .remote : .local
        }
    #endif
}
