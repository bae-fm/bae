/// Encoding for the album ids a grid card carries in a drag: a marker line, then
/// the albums, one per line, in no order. A drop hands them to core, which
/// orders them as the grid lists them. SwiftUI's `.draggable` yields one
/// `String` per view, so the ids join with a newline — album ids are UUIDs, so a
/// newline never occurs inside one — and the marker tells an album drag from
/// a dropped track id.
enum AlbumDragPayload {
    private static let marker = "bae-albums"

    static func encode(_ ids: [String]) -> String {
        ([marker] + ids).joined(separator: "\n")
    }

    /// The album ids `payload` carries, or nil for a string no album drag
    /// made, such as a dragged track's id.
    static func albums(in payload: String) -> [String]? {
        let lines =
            payload.split(separator: "\n", omittingEmptySubsequences: true)
            .map(String.init)
        guard lines.first == marker else { return nil }
        return Array(lines.dropFirst())
    }
}
