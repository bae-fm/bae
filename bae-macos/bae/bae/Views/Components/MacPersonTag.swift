import BaeKit
import SwiftUI

/// A person's small picture, or initials, beside their name, as a history row
/// ends in whose play it was. `strong` is a playlist header's owner line: the
/// name in semibold at the size of the text around it.
struct MacPersonTag: View {
    let person: BridgePersonRef
    var strong = false

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            Avatar(
                initials: person.initials,
                picture: person.avatarVersion.map { (person.person, $0) },
                size: strong ? 16 : 18
            )
            Text(verbatim: person.name)
                .themeText(strong ? .strong : .detail)
                .lineLimit(1)
        }
        .foregroundStyle(Theme.macTextSecondary)
    }
}
