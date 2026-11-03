import BaeKit

extension BridgeSaveFormat {
    /// The format's name as core gives it, a proper noun shown as-is in every
    /// locale.
    var label: String { bridgeSaveFormatName(format: self) }
}
