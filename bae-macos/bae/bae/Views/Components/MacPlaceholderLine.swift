import BaeKit
import SwiftUI

/// A grey bar where a line of text will be, or with a `radius` of a picture's
/// corners, a grey square where a picture will be.
struct MacPlaceholderLine: View {
    let width: CGFloat
    let height: CGFloat
    var radius = ThemeRadius.bar

    var body: some View {
        RoundedRectangle(cornerRadius: radius)
            .fill(Theme.placeholder)
            .frame(width: width, height: height)
    }
}
