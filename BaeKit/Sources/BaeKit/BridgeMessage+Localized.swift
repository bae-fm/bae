import Foundation

extension BridgeMessage {
    /// The message in the person's language, its arguments formatted in by
    /// the position the `Core` table gives each one.
    public var localizedLine: String {
        String(
            format: localizedCoreString(key),
            locale: Locale.current,
            arguments: args.map(\.value)
        )
    }
}
