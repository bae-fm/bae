import BaeKit

/// The places a scene states for the rows of a list: each scene's rows are
/// fixed data, as core would hand them.
extension BridgeListPlace {
    /// The one blank row of a list that holds none.
    static let blankAlone = BridgeListPlace(
        labelled: true,
        removable: false,
        addsRow: true
    )
    /// The only row of a list, holding a value.
    static let filledAlone = BridgeListPlace(
        labelled: true,
        removable: true,
        addsRow: true
    )
    /// The first of several rows.
    static let first = BridgeListPlace(
        labelled: true,
        removable: true,
        addsRow: false
    )
    /// The last of several rows.
    static let last = BridgeListPlace(
        labelled: false,
        removable: true,
        addsRow: true
    )
}

extension BridgeLabelRow {
    /// The one blank row of a pressing with no label.
    static let blank = BridgeLabelRow(
        label: BridgeRawLabelEdit(name: "", catalogNumber: ""),
        place: .blankAlone
    )

    /// The only label row of a pressing.
    static func alone(name: String, catalogNumber: String) -> BridgeLabelRow {
        BridgeLabelRow(
            label: BridgeRawLabelEdit(name: name, catalogNumber: catalogNumber),
            place: .filledAlone
        )
    }
}

extension BridgeBarcodeRow {
    /// The one blank row of a pressing with no barcode.
    static let blank = BridgeBarcodeRow(barcode: "", place: .blankAlone)

    /// The only barcode row of a pressing.
    static func alone(_ barcode: String) -> BridgeBarcodeRow {
        BridgeBarcodeRow(barcode: barcode, place: .filledAlone)
    }
}
