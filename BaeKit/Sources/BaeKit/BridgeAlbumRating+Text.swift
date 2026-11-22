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

    /// Whether this person has a heart on the album: they rated it three
    /// stars or more.
    public var isHearted: Bool {
        mine?.isHeart == true
    }

    /// The stars the Heart or Remove heart item asks for: five stars turn
    /// the heart on, and the person's rating goes when it is already on.
    public var starsAfterHeartToggle: BridgeStars? {
        isHearted ? nil : .five
    }

    /// Who has a heart on the album in the viewer's language: "You" for
    /// this person, then "Ana hearted it" for each other, in the order core
    /// read them. Nobody who rated it under three stars has a heart.
    public var heartersText: String {
        FactList.joined(
            raters.compactMap { rater in
                switch rater {
                case .you:
                    isHearted ? String(localized: "You") : nil
                case .person(let name, let stars):
                    stars.isHeart
                        ? String(localized: "\(name) hearted it") : nil
                }
            }
        )
    }
}
