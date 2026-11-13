import Foundation

extension BridgeCreditRow {
    /// The row's artists in the viewer's language, followed by the tracks it
    /// covers ("Artist Name (tracks 2 and 5)"); a row covering every track
    /// names none.
    public var text: String {
        guard !tracks.isEmpty else {
            return artists.text
        }
        let list = ListFormatter.localizedString(byJoining: tracks)
        return artists.text + " " + String(localized: "(tracks \(list))")
    }
}
