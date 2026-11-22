import Foundation

extension [BridgeRater] {
    /// Who rated the album in the viewer's language: "You" for this person,
    /// then "Ana rated it 5" for each other, in the order core read them.
    public var text: String {
        FactList.joined(
            map { rater in
                switch rater {
                case .you:
                    String(localized: "You")
                case .person(let name, let stars):
                    String(localized: "\(name) rated it \(stars.number)")
                }
            }
        )
    }
}

extension [BridgeHearter] {
    /// Who has a heart on the album in the viewer's language: "You" for this
    /// person, then "Ana hearted it" for each other, in the order core read
    /// them.
    public var text: String {
        FactList.joined(
            map { hearter in
                switch hearter {
                case .you:
                    String(localized: "You")
                case .person(let name):
                    String(localized: "\(name) hearted it")
                }
            }
        )
    }
}

extension BridgeStars {
    /// The stars a tap on this star asks for when the person's rating is
    /// `mine`: the same stars again clears it.
    public func afterTapping(from mine: BridgeStars?) -> BridgeStars? {
        self == mine ? nil : self
    }
}
