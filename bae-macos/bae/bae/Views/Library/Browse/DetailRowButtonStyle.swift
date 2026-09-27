import BaeKit
import SwiftUI

/// The detail pane's row buttons, whose hover fill bleeds past the content so
/// the text stays column-aligned.
struct DetailRowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        DetailRow(configuration: configuration)
    }

    private struct DetailRow: View {
        let configuration: Configuration
        @State
        private var isHovered = false

        var body: some View {
            configuration.label
                .padding(.vertical, ThemeSpace.compact)
                .padding(.horizontal, ThemeSpace.related)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(
                    RoundedRectangle(cornerRadius: ThemeRadius.control)
                        .fill(fill)
                )
                .contentShape(Rectangle())
                .onHover { isHovered = $0 }
                .padding(.horizontal, -ThemeSpace.related)
        }

        private var fill: Color {
            if configuration.isPressed {
                return Theme.pressed
            }
            return isHovered ? Theme.hover : .clear
        }
    }
}

#if DEBUG
    #Preview("Detail Row Button Style") {
        VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
            Button(action: {}) { Text(verbatim: "Sample Row") }
                .buttonStyle(DetailRowButtonStyle())
            Button(action: {}) { Text(verbatim: "Another Row") }
                .buttonStyle(DetailRowButtonStyle())
        }
        .padding()
        .frame(width: 360, alignment: .leading)
    }
#endif
