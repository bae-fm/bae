import BaeKit
import SwiftUI

/// How many releases a lookup named, as a small capsule: green when it named
/// any, plain when it named none — or when the number is not a match count
/// at all, like how many catalog numbers there are to choose from.
struct CountCapsule: View {
    let text: String
    let matched: Bool

    init(text: String, matched: Bool) {
        self.text = text
        self.matched = matched
    }

    /// A match count: green when there is at least one.
    init(count: Int) {
        self.init(text: count.formatted(), matched: count > 0)
    }

    var body: some View {
        Text(text)
            .font(.system(size: 10.5, weight: .semibold))
            .monospacedDigit()
            .foregroundStyle(matched ? Color.green : Color.secondary)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(
                matched ? Color.green.opacity(0.14) : Theme.hover,
                in: RoundedRectangle(cornerRadius: 4)
            )
    }
}
