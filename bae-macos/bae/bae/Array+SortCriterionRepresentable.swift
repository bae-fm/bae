import BaeKit
import Foundation
import os.log

private let logger = Logger.bae("Array+SortCriterionRepresentable")

/// A sort field with a stable, locale-free string identity — the vocabulary
/// UserDefaults persistence round-trips through. Shared by the album,
/// composer, and artist sort fields so the codec below covers all three
/// modes without a per-mode copy.
protocol SortCriterionFieldCodable {
    var codableKey: String { get }
    static func fromCodableKey(_ key: String) -> Self?
}

extension BridgeSortField: SortCriterionFieldCodable {}
extension BridgeComposerSortField: SortCriterionFieldCodable {}
extension BridgeArtistSortField: SortCriterionFieldCodable {}

/// Manual JSON since UniFFI types can't auto-synthesize Codable from another
/// file. Generic over any sort-criteria list — album, composer, artist — so
/// the three modes share one persistence implementation.
extension Array
where
    Element: SortCriterionRepresentable,
    Element.Field: SortCriterionFieldCodable
{
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

    static func fromJSON(_ data: Data) -> [Element]? {
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
                    .flatMap(Element.Field.fromCodableKey),
                let direction = dict["direction"]
                    .flatMap(BridgeSortDirection.fromCodableKey)
            else {
                return nil
            }
            return Element(field: field, direction: direction)
        }
    }
}
