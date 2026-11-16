import BaeKit
import SwiftUI

/// Where a dragged row will land in a list the person reorders: a line in
/// the accent colour over the gap between two rows, which takes no clicks.
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
