import BaeKit

extension BridgeRawPressingEdit {
    /// A pressing whose label and barcode rows are laid out by core, for a
    /// form bound to a value rather than to a draft in core.
    init(
        year: String,
        storedLabels: [BridgeRawLabelEdit],
        storedBarcodes: [String],
        facts: BridgePressingFacts
    ) {
        self.init(
            year: year,
            labels: bridgeLabelRows(labels: storedLabels),
            barcodes: bridgeBarcodeRows(barcodes: storedBarcodes),
            facts: facts
        )
    }
}

extension BridgeRawPressingEdit {
    /// The pressing after a write of every label row, laid out by core.
    mutating func set(labels: [BridgeRawLabelEdit]) {
        self.labels = bridgeLabelRows(labels: labels)
    }

    /// The pressing after a write of every barcode row, laid out by core.
    mutating func set(barcodes: [String]) {
        self.barcodes = bridgeBarcodeRows(barcodes: barcodes)
    }
}
