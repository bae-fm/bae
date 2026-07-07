import BaeKit
import Foundation

extension BridgeSortDirection {
    var codableKey: String {
        switch self {
        case .ascending: "ascending"
        case .descending: "descending"
        }
    }

    static func fromCodableKey(_ key: String) -> BridgeSortDirection? {
        key == "ascending"
            ? .ascending : key == "descending" ? .descending : nil
    }
}
