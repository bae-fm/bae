import BaeKit
import SwiftUI

/// A person's picture: their avatar once the profile has one, else their
/// initials in the accent on the strong accent tint. Round.
struct Avatar: View {
    let name: String
    /// The person and the version of their avatar, when they have one.
    var picture: (person: String, version: String)?
    let size: CGFloat

    var body: some View {
        ZStack {
            Circle().fill(Theme.accentStrong)
            Text(verbatim: Self.initials(of: name))
                .font(.system(size: size * 0.4, weight: .semibold))
                .foregroundStyle(Theme.accent)
            if let picture {
                ImageView(
                    content: .avatar(
                        person: picture.person,
                        version: picture.version
                    ),
                    pointSize: size
                )
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .accessibilityHidden(true)
    }

    /// The first letter of up to two words, upper case.
    static func initials(of name: String) -> String {
        name.split(whereSeparator: \.isWhitespace)
            .prefix(2)
            .compactMap { $0.first.map { String($0).uppercased() } }
            .joined()
    }
}
