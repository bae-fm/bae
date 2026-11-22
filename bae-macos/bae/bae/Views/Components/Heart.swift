import SwiftUI

/// A rating in a library whose rating system is Heart, read-only: one heart,
/// filled in the accent when `on` (the person gave three stars or more), an
/// outline otherwise.
struct Heart: View {
    var on = false
    let size: CGFloat

    var body: some View {
        Image(systemName: on ? "heart.fill" : "heart")
            .font(.system(size: size))
            .foregroundStyle(on ? Theme.accent : Theme.hairlineStrong)
            .frame(width: size, height: size)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(String(localized: "Hearted"))
            .accessibilityHidden(!on)
    }
}
