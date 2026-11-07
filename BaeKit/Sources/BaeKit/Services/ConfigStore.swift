import SwiftUI

/// Mirror of core's library configuration. The config value stream is its only
/// writer.
@Observable
public class ConfigStore {
    public var config: Config
    public private(set) var storage: BridgeStorageLocation?

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
