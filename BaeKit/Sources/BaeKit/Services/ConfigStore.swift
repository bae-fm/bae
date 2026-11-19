import SwiftUI

/// Mirror of core's library configuration. The config value stream is its only
/// writer.
@Observable
public class ConfigStore {
    public var config: Config

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
