import Foundation

extension BridgeArtistSortField: CaseIterable {
    public static var allCases: [BridgeArtistSortField] {
        [.name, .albumCount]
    }

    public var displayName: String {
        switch self {
        case .name: String(localized: "Name")
        case .albumCount: String(localized: "Albums")
        }
    }

    public var codableKey: String {
        switch self {
        case .name: "name"
        case .albumCount: "albumCount"
        }
    }

    public static func fromCodableKey(_ key: String) -> BridgeArtistSortField? {
        allCases.first { $0.codableKey == key }
    }
}
