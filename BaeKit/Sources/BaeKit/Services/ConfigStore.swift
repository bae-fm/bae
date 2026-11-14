import SwiftUI

/// Mirror of core's library configuration. The config value stream is its only
/// writer.
@Observable
public class ConfigStore {
    public var config: Config
    public private(set) var storage: BridgeStorageLocation?
    /// On-screen state, not config: the id of the save format the person just
    /// added in Settings, which an open save panel's format pop-up chooses
    /// and then clears.
    public var addedSavePresetId: String?

    public var syncs: Bool { storage != nil }

    public func applyStorage(_ location: BridgeStorageLocation?) {
        storage = location
    }

    public init(
        config: Config,
        storage: BridgeStorageLocation?
    ) {
        self.storage = storage
        self.config = config
    }

    public func applyConfigSnapshot(_ config: BridgeConfig) {
        self.config = Config(bridge: config)
    }
}
