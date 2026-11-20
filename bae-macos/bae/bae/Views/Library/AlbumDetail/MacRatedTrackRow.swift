import BaeKit
import SwiftUI

/// A track row of an album's list with the slot after it for the stars the
/// person gave the track. The slot stays empty for a track they did not
/// rate, so every row's duration lines up.
struct MacRatedTrackRow: View {
    let row: MacTrackRow
    let stars: BridgeStars?

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            row
            Stars(value: stars?.number ?? 0, size: 10)
                .opacity(stars == nil ? 0 : 1)
                .accessibilityHidden(stars == nil)
        }
    }
}
