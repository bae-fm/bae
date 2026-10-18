import Foundation

extension BridgeUnsentWork {
    /// One sentence for each kind of change waiting, in the person's language.
    public var lines: [String] {
        [
            (writes, "core.library.removal.unsent_writes"),
            (files, "core.library.removal.unsent_files"),
        ]
        .filter { count, _ in count > 0 }
        .map { count, key in
            String.localizedStringWithFormat(
                localizedCoreString(key),
                Int(count)
            )
        }
    }

    /// A removal confirmation's text: what removing would lose, then
    /// `consequence` saying removing loses it, then `message`. Only `message`
    /// when nothing waits to upload.
    public func confirmation(consequence: String, message: String) -> String {
        let lost = lines
        return (lost.isEmpty ? [message] : lost + [consequence, message])
            .joined(separator: " ")
    }
}
