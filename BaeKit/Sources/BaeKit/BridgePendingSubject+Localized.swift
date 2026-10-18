import Foundation

extension BridgePendingSubject {
    /// What a pending record is about, in the person's language: a file by
    /// its name and album, anything else by what it is.
    public var localizedLine: String {
        let format = localizedCoreString(bridgePendingSubjectKey(subject: self))
        switch self {
        case .releaseFile(let fileName, let albumTitle):
            return String(
                format: format,
                locale: Locale.current,
                fileName,
                albumTitle
            )
        case .cover(let albumTitle):
            return String(format: format, locale: Locale.current, albumTitle)
        default:
            return format
        }
    }
}
