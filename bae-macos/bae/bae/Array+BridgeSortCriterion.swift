import Foundation
import os.log

private let logger = Logger.bae("Array+BridgeSortCriterion")

/// Manual JSON since UniFFI types can't auto-synthesize Codable from another file
extension [BridgeSortCriterion] {
    func toJSON() -> Data? {
        let dicts = map {
            [
                "field": $0.field.codableKey,
                "direction": $0.direction.codableKey,
            ]
        }
        do {
            return try JSONSerialization.data(withJSONObject: dicts)
        }
        catch {
            logger.error(
                "Failed to encode sort criteria: \(error.localizedDescription)"
            )
            return nil
        }
    }

    static func fromJSON(_ data: Data) -> [BridgeSortCriterion]? {
        let raw: Any
        do {
            raw = try JSONSerialization.jsonObject(with: data)
        }
        catch {
            logger.error(
                "Failed to decode sort criteria: \(error.localizedDescription)"
            )
            return nil
        }
        guard let array = raw as? [[String: String]] else {
            logger.error("Sort criteria JSON has unexpected shape")
            return nil
        }
        return array.compactMap { dict in
            guard
                let field = dict["field"]
                    .flatMap(BridgeSortField.fromCodableKey),
                let direction = dict["direction"]
                    .flatMap(BridgeSortDirection.fromCodableKey)
            else {
                return nil
            }
            return BridgeSortCriterion(field: field, direction: direction)
        }
    }
}
