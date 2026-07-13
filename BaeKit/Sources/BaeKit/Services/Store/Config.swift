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
    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. A synced preference, so the bar
    /// reads it here rather than keeping its own copy per device.
    public let showRemainingTime: Bool
    /// Where release exports write: prompt each time, or a fixed folder.
    public let exportLocation: BridgeExportLocation
    /// Template for the default filename a single-track export suggests. Core
    /// renders the tokens; the UI edits the raw string.
    public let exportFilenameTemplate: String
    /// Configured export presets offered by release and track export.
    public let exportPresets: [BridgeExportPreset]
    /// Default selected option in the track export picker.
    public let defaultTrackExportSelection: BridgeExportSelection
    /// Default selected option in the release export picker.
    public let defaultReleaseExportSelection: BridgeExportSelection
    public let mcp: BridgeMcpConfig

    public var hasCloudHome: Bool { sync != nil }

    public init(bridge: BridgeConfig) {
        libraryId = bridge.libraryId
        libraryName = bridge.libraryName
        libraryPath = bridge.libraryPath
        discogsTokenStatus = bridge.discogsTokenStatus
        discogsUsable = bridge.discogsUsable
        sync = bridge.sync
        pauseBetweenSides = bridge.pauseBetweenSides
        showRemainingTime = bridge.showRemainingTime
        exportLocation = bridge.exportLocation
        exportFilenameTemplate = bridge.exportFilenameTemplate
        exportPresets = bridge.exportPresets
        defaultTrackExportSelection = bridge.defaultTrackExportSelection
        defaultReleaseExportSelection = bridge.defaultReleaseExportSelection
        mcp = bridge.mcp
    }

    /// The storage state to import into. `Managed` only when a cloud home
    /// exists and the user chose it; otherwise `Unmanaged`. Whether to keep the
    /// release pinned (offline) is the orthogonal `pin` argument to
    /// `startImport`, never folded in here.
    public func importStorageMode(managed: Bool) -> BridgeStorageMode {
        hasCloudHome && managed ? .remote : .local
    }
}
