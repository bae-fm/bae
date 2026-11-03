import Foundation

extension BridgeArtistSortField: CaseIterable {
    public static var allCases: [BridgeArtistSortField] {
        [.name, .albumCount]
    }

    public var displayName: LocalizedStringResource {
        switch self {
        case .name: "Name"
        case .albumCount: "Albums"
        }
    }
}
