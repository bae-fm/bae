import BaeKit
import SwiftUI

/// A count in a small capsule, in the success color when `matched`.
struct CountCapsule: View {
    let text: String
    let matched: Bool

    init(text: String, matched: Bool) {
        self.text = text
        self.matched = matched
    }

    /// A match count, marked matched when there is at least one.
    init(count: Int) {
        self.init(text: count.formatted(), matched: count > 0)
    }

    var body: some View {
        Text(text)
            .themeText(.chip)
            .monospacedDigit()
            .foregroundStyle(matched ? Theme.success : Color.secondary)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(
                matched
                    ? Theme.success.opacity(ThemeOpacity.tint) : Theme.hover,
                in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
            )
    }
}
