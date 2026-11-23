import Foundation

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
