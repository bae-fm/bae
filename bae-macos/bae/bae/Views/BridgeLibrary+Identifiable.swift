import Foundation

/// `BridgeLibrary` already carries an `id: String`. Declaring conformance
/// lets `.sheet(item:)` / `ForEach` drop the explicit `id: \.id`.
extension BridgeLibrary: Identifiable {}
