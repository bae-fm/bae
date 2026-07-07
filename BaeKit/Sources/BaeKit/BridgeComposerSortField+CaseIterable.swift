import Foundation

extension BridgeComposerSortField: CaseIterable {
    public static var allCases: [BridgeComposerSortField] {
        [.name, .workCount, .linkedReleaseCount]
    }

    public var displayName: String {
        switch self {
        case .name: String(localized: "Name")
        case .workCount: String(localized: "Works")
        case .linkedReleaseCount: String(localized: "Releases")
        }
    }
}
