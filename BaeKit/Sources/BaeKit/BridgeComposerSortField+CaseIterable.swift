import Foundation

extension BridgeComposerSortField: CaseIterable {
    public static var allCases: [BridgeComposerSortField] {
        [.name, .workCount, .linkedReleaseCount]
    }

    public var displayName: LocalizedStringResource {
        switch self {
        case .name: "Name"
        case .workCount: "Works"
        case .linkedReleaseCount: "Releases"
        }
    }
}
