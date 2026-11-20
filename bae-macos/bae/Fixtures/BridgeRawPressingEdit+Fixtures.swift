import BaeKit

/// The places a scene states for the rows of a list: each scene's rows are
/// fixed data, as core would hand them.
extension BridgeListPlace {
    /// The only row of a list, holding a value.
    static let filledAlone = BridgeListPlace(
        labelled: true,
        addsRow: true
    )
    /// The first of several rows.
    static let first = BridgeListPlace(
        labelled: true,
        addsRow: false
    )
    /// The last of several rows.
    static let last = BridgeListPlace(
        labelled: false,
        addsRow: true
    )
}

extension BridgeLabelRow {
    /// The only label row of a pressing.
    static func alone(name: String, catalogNumber: String) -> BridgeLabelRow {
        BridgeLabelRow(
            label: BridgeRawLabelEdit(name: name, catalogNumber: catalogNumber),
            place: .filledAlone
        )
    }
}

extension BridgeBarcodeRow {
    /// The only barcode row of a pressing.
    static func alone(_ barcode: String) -> BridgeBarcodeRow {
        BridgeBarcodeRow(barcode: barcode, place: .filledAlone)
    }
}
