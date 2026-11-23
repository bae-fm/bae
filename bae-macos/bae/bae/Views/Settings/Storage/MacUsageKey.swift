import BaeKit
import SwiftUI

/// One kind in a storage legend: its colour's dot, its name and its size.
struct MacUsageKey: View {
    let color: Color
    let title: String
    let size: String

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            Circle()
                .fill(color)
                .frame(width: 8, height: 8)
            Text(verbatim: title)
                .foregroundStyle(Theme.textSecondary)
            Text(verbatim: size)
                .monospacedDigit()
        }
        .themeText(.detail)
        .fixedSize()
    }
}
