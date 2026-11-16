import Foundation

extension BridgeAlbumRating {
    /// Who rated the album in the viewer's language: "You" for this person,
    /// then "Ana rated it 5" for each other, in the order core read them.
    public var ratersText: String {
        FactList.joined(
            raters.map { rater in
                switch rater {
                case .you:
                    String(localized: "You")
                case .person(let name, let stars):
                    String(localized: "\(name) rated it \(stars.number)")
                }
            }
        )
    }

    /// The stars a tap on the `tapped` star asks for: the same stars again
    /// clears the person's rating.
    public func stars(afterTapping tapped: BridgeStars) -> BridgeStars? {
        tapped == mine ? nil : tapped
    }
}
