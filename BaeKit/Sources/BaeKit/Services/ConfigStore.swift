import SwiftUI

/// Mirror of core's library configuration. The config value stream is its only
/// writer.
@Observable
public class ConfigStore {
    public var config: Config
    /// On-screen state, not config: the id of the save format the person just
    /// added in Settings, which an open save panel's format pop-up chooses
    /// and then clears.
    public var addedSavePresetId: String?

    /// Where the library is stored, as core's config says.
    public var storage: BridgeStorageLocation? { config.storage }

    public var syncs: Bool { storage != nil }

    public init(config: Config) {
        self.config = config
    }

    public func applyConfigSnapshot(_ config: BridgeConfig) {
        self.config = Config(bridge: config)
    }
}
