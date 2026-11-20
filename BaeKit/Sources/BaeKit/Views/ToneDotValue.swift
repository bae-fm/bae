import SwiftUI

/// A row's value in its tone: a dot and the words in the tone's colour, and
/// plain secondary words for the neutral tone, which has no dot.
public struct ToneDotValue: View {
    public let text: String
    public let tone: StatusTone

    public init(text: String, tone: StatusTone) {
        self.text = text
        self.tone = tone
    }

    public var body: some View {
        HStack(spacing: ThemeSpace.compact) {
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
