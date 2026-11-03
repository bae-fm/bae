import Foundation

extension BridgeCountLabel {
    /// The label in the person's language: its `Core` message with the count
    /// formatted in, plural and all.
    public var localizedLine: String {
        String.localizedStringWithFormat(localizedCoreString(key), Int(count))
    }
}
