import BaeKit
import SwiftUI

/// A grey bar where a line of text will be.
struct MacPlaceholderLine: View {
    let width: CGFloat
    let height: CGFloat

    var body: some View {
        RoundedRectangle(cornerRadius: ThemeRadius.bar)
            .fill(Theme.placeholder)
            .frame(width: width, height: height)
    }
}
