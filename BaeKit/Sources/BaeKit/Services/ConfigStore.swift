import SwiftUI

/// Mirror of core's library configuration. The config value stream is its only
/// writer.
@Observable
public class ConfigStore {
    public var config: Config
    public private(set) var libraryName: String
    public private(set) var storage: BridgeStorageLocation?

    public var hasCloudHome: Bool { storage != nil }

    public func applyLibraryName(_ name: String) { libraryName = name }
    public func applyStorage(_ location: BridgeStorageLocation?) {
        storage = location
    }

    public init(
        config: Config,
        libraryName: String,
        storage: BridgeStorageLocation?
    ) {
        self.libraryName = libraryName
        self.storage = storage
        self.config = config
    }

    public func applyConfigSnapshot(_ config: BridgeConfig) {
        self.config = Config(bridge: config)
    }

}
