/// Encoding for the album ids a grid card carries in a drag. A single card drags
/// one id; a card that's part of a multi-selection drags the whole ordered
/// selection. SwiftUI's `.draggable` yields one `String` per view, so several ids
/// join with a newline — album ids are UUIDs, so a newline never occurs inside
/// one. The queue drop sites decode every dropped string back through `decode`,
/// which leaves a single-id payload as one id.
enum AlbumDragPayload {
    static func encode(_ ids: [String]) -> String {
        ids.joined(separator: "\n")
    }

    static func decode(_ payload: String) -> [String] {
        payload
            .split(separator: "\n", omittingEmptySubsequences: true)
            .map(String.init)
    }
}
