import BaeKit
import SwiftUI

/// A person's picture: their avatar once the profile has one, else their
/// initials in the accent on the strong accent tint. Round.
struct Avatar: View {
    /// The letters drawn while there is no picture.
    let initials: String
    /// The person and the version of their avatar, when they have one.
    var picture: (person: String, version: String)?
    let size: CGFloat
    /// Whether screen readers skip the picture. A picture that is the whole
    /// face of a button is not decorative: the button's name needs it.
    var isDecorative = true

    var body: some View {
        ZStack {
            Circle().fill(Theme.accentStrong)
            Text(verbatim: initials)
                .font(.system(size: size * 0.4, weight: .semibold))
                .foregroundStyle(Theme.accent)
            if let picture {
                Artwork(
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
        .accessibilityHidden(isDecorative)
    }
}

extension Avatar {
    /// A person's picture from their profile: the avatar their profile names,
    /// or their initials while it names none.
    init(
        profile: BridgePersonProfile,
        size: CGFloat,
        isDecorative: Bool = true
    ) {
        self.init(
            initials: profile.initials,
            picture: profile.avatarVersion.map { (profile.id, $0) },
            size: size,
            isDecorative: isDecorative
        )
    }
}
