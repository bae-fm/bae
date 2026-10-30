import Foundation

// CaseIterable powers picker UI.

extension BridgeSortField: CaseIterable {
    public static var allCases: [BridgeSortField] {
        [.title, .artist, .year, .dateAdded]
    }

    public var displayName: String {
        switch self {
        case .title: "Title"
        case .artist: "Artist"
        case .year: "Year"
        case .dateAdded: "Date Added"
        }
    }
}
