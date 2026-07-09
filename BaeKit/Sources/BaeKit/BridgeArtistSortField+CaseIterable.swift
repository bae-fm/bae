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
}
