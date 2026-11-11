import Foundation

// MARK: - Config

public struct Config: Equatable {
    public let libraryId: String
    public let libraryPath: String
    public let discogsTokenStatus: BridgeDiscogsTokenStatus
    /// Whether Discogs can be used as a metadata source. Core decides the policy
    /// (a stored key that isn't rejected); the UI reads this, not the status.
    public let discogsUsable: Bool
    public let pauseBetweenSides: Bool
    /// Whether a side or disc pause ends on its own after a countdown, and how
    /// long. Settings show it only while `pauseBetweenSides` is on.
    public let sidePauseCountdown: BridgeSidePauseCountdown
    /// How many file uploads run at once, and how many downloads a pin fetches at
    /// once. Device-local (a per-machine choice), 1 up to
    /// `maxConcurrentTransfers`. The storage manager reads them here and writes
    /// through the bridge setters.
    public let maxConcurrentUploads: UInt32
    public let maxConcurrentDownloads: UInt32
    /// The most uploads, and the most downloads, core lets run at once.
    public let maxConcurrentTransfers: UInt32
    /// Whether identification starts on its own: a newly added candidate is
    /// identified as it arrives.
    public let identifyAutomatically: Bool
    /// Whether what an identification run settles on as needing nothing is
    /// imported straight away, whether the run started on its own or a person
    /// asked for it. Independent of `identifyAutomatically`.
    public let importWhenIdentified: Bool
    /// Where an import goes, shown and changed in the import pane and
    /// Settings; core reads it as each import starts.
    public let importDestination: BridgeImportDestination
    /// Which catalogs Find online asks, one entry per catalog in core's order.
    /// Core folds the person's choice and the catalog's credentials into one
    /// availability, so the switches on the Find online header and in Settings
    /// render this rather than re-deriving "on and reachable" from
    /// `discogsUsable` plus a flag of their own.
    public let lookupCatalogs: [BridgeLookupCatalogSetting]
    /// Whether any catalog is asked, core's answer: with none, Find online
    /// has nothing to search and says so.
    public let hasSourceToSearch: Bool
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
    /// The one file name pattern every save names its files by, whatever the
    /// format, as typed.
    public let filenamePieces: [BridgeSaveFilenamePiece]
    /// Why the file name pattern can't make a name, when it can't; saves name
    /// by the default pattern until it is fixed.
    public let filenamePatternError: BridgeNamePatternError?
    /// The folder name pattern a release save names its folder by, as typed.
    public let folderPieces: [BridgeSaveFolderPiece]
    /// Why the folder name pattern can't make a name, when it can't.
    public let folderPatternError: BridgeNamePatternError?
    /// The cover and loudness settings every save format shares.
    public let saveExtras: BridgeSaveExtras
    /// Id of the preset the last save, of a track or a release, used: the one
    /// a save starts with.
    public let lastSavePreset: String
    /// Whether casting to a network receiver is available. Core enforces it —
    /// while off it runs no discovery and refuses to start a session — so the
    /// playback bar reads this only to decide whether to show its Cast control.
    public let castEnabled: Bool
    /// What a track a Cast or UPnP device can't play directly is converted
    /// to. Core applies it per device flavor, from the next track loaded.
    public let castTranscodeFormat: BridgeCastTranscodeFormat
    public let mcp: BridgeMcpConfig
    public let subsonic: BridgeSubsonicConfig

    public init(bridge: BridgeConfig) {
        libraryId = bridge.libraryId
        libraryPath = bridge.libraryPath
        discogsTokenStatus = bridge.discogsTokenStatus
        discogsUsable = bridge.discogsUsable
        pauseBetweenSides = bridge.pauseBetweenSides
        sidePauseCountdown = bridge.sidePauseCountdown
        maxConcurrentUploads = bridge.maxConcurrentUploads
        maxConcurrentDownloads = bridge.maxConcurrentDownloads
        maxConcurrentTransfers = bridge.maxConcurrentTransfers
        identifyAutomatically = bridge.identifyAutomatically
        importWhenIdentified = bridge.importWhenIdentified
        importDestination = bridge.importDestination
        lookupCatalogs = bridge.lookupCatalogs
        hasSourceToSearch = bridge.hasSourceToSearch
        showRemainingTime = bridge.showRemainingTime
        libraryFullWidth = bridge.libraryFullWidth
        savePresets = bridge.savePresets
        filenamePieces = bridge.filenamePieces
        filenamePatternError = bridge.filenamePatternError
        folderPieces = bridge.folderPieces
        folderPatternError = bridge.folderPatternError
        saveExtras = bridge.saveExtras
        lastSavePreset = bridge.lastSavePreset
        castEnabled = bridge.castEnabled
        castTranscodeFormat = bridge.castTranscodeFormat
        mcp = bridge.mcp
        subsonic = bridge.subsonic
    }
}
