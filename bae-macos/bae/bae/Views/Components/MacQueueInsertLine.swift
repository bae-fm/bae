import BaeKit
import SwiftUI

/// Shows where the person will drop a dragged row in a list they reorder: a
/// line in the accent colour over the gap between two rows, which ignores
/// clicks.
/// The list places it over the gap, so it takes no room and no row moves.
struct MacQueueInsertLine: View {
    var body: some View {
        Rectangle()
            .fill(Theme.accent)
            .frame(height: 2)
            .padding(.horizontal, ThemeSpace.related)
            .allowsHitTesting(false)
    }
}
