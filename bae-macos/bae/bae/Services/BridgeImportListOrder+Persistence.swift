import BaeKit

extension BridgeImportListOrder {
    var preferenceValue: String {
        switch self {
        case .newestFirst: "newestFirst"
        case .oldestFirst: "oldestFirst"
        case .pathAscending: "nameAZ"
        case .pathDescending: "nameZA"
        }
    }

    init?(preferenceValue: String) {
        switch preferenceValue {
        case "newestFirst": self = .newestFirst
        case "oldestFirst": self = .oldestFirst
        case "nameAZ": self = .pathAscending
        case "nameZA": self = .pathDescending
        default: return nil
        }
    }
}
