import Foundation

/// The changes this device hasn't sent, which stop a library's removal until
/// the person discards them: edits waiting to upload and files that haven't
/// finished uploading.
public struct UnsentEdits: Equatable, Sendable {
    public let writes: UInt64
    public let files: UInt64

    public init(writes: UInt64, files: UInt64) {
        self.writes = writes
        self.files = files
    }

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
}
