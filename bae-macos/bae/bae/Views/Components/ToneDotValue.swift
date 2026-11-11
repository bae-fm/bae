import BaeKit
import SwiftUI

/// A row's value in its tone: a dot and the words in the tone's colour, and
/// plain secondary words for the neutral tone, which has no dot.
struct ToneDotValue: View {
    let text: String
    let tone: StatusTone

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            if tone != .neutral {
                Circle()
                    .fill(tone.color)
                    .frame(width: 7, height: 7)
            }
            Text(text)
                .foregroundStyle(tone.color)
        }
    }
}
