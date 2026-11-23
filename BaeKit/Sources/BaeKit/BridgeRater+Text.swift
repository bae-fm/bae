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
