import BaeKit

extension BridgeListPlace {
    /// The place of row `index` among `count` rows, as a form draws a list
    /// whose rows a scene states: the first row is labelled, the last adds a
    /// row, and a lone blank row has nothing to remove.
    fileprivate init(index: Int, count: Int, blank: Bool) {
        self.init(
            labelled: index == 0,
            removable: count > 1 || !blank,
            addsRow: index == count - 1
        )
    }
}

extension BridgeRawPressingEdit {
    /// A pressing whose label and barcode rows a scene states: one blank row
    /// where it states none.
    init(
        year: String,
        storedLabels: [BridgeRawLabelEdit],
        storedBarcodes: [String],
        facts: BridgePressingFacts
    ) {
        self.init(
            year: year,
            labels: Self.labelRows(storedLabels),
            barcodes: Self.barcodeRows(storedBarcodes),
            facts: facts
        )
    }

    /// The pressing after a write of every label row.
    mutating func set(labels: [BridgeRawLabelEdit]) {
        self.labels = Self.labelRows(labels)
    }

    /// The pressing after a write of every barcode row.
    mutating func set(barcodes: [String]) {
        self.barcodes = Self.barcodeRows(barcodes)
    }

    private static func labelRows(_ stored: [BridgeRawLabelEdit])
        -> [BridgeLabelRow]
    {
        let shown =
            stored.isEmpty
            ? [BridgeRawLabelEdit(name: "", catalogNumber: "")] : stored
        return shown.enumerated()
            .map { index, label in
                BridgeLabelRow(
                    label: label,
                    place: BridgeListPlace(
                        index: index,
                        count: shown.count,
                        blank: label.name.isEmpty && label.catalogNumber.isEmpty
                    )
                )
            }
    }

    private static func barcodeRows(_ stored: [String]) -> [BridgeBarcodeRow] {
        let shown = stored.isEmpty ? [""] : stored
        return shown.enumerated()
            .map { index, barcode in
                BridgeBarcodeRow(
                    barcode: barcode,
                    place: BridgeListPlace(
                        index: index,
                        count: shown.count,
                        blank: barcode.isEmpty
                    )
                )
            }
    }
}
