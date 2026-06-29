import Foundation

extension BridgeComposerSortField: CaseIterable {
    public static var allCases: [BridgeComposerSortField] {
        [.name, .workCount, .linkedReleaseCount]
    }

    var displayName: String {
        switch self {
        case .name: String(localized: "Name")
        case .workCount: String(localized: "Works")
        case .linkedReleaseCount: String(localized: "Releases")
        }
    }
}
