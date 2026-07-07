import Foundation

// CaseIterable powers picker UI; codableKey backs UserDefaults persistence.

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

    public var codableKey: String {
        switch self {
        case .title: "title"
        case .artist: "artist"
        case .year: "year"
        case .dateAdded: "dateAdded"
        }
    }

    public static func fromCodableKey(_ key: String) -> BridgeSortField? {
        allCases.first { $0.codableKey == key }
    }
}
