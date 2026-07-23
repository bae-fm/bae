import SwiftUI

/// The detail pane's row buttons: 6/8 padding, radius-8 rounding, a hover wash,
/// and a negative horizontal margin so the hover fill bleeds to the pane's
/// content edges while the content itself stays column-aligned.
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
                .padding(.vertical, 6)
                .padding(.horizontal, 8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(fill)
                )
                .contentShape(Rectangle())
                .onHover { isHovered = $0 }
                .padding(.horizontal, -8)
        }

        private var fill: Color {
            if configuration.isPressed {
                return Color.primary.opacity(0.08)
            }
            return isHovered ? Color.primary.opacity(0.04) : .clear
        }
    }
}

#if DEBUG
    #Preview("Detail Row Button Style") {
        VStack(alignment: .leading, spacing: 2) {
            Button("Sample Row") {}
                .buttonStyle(DetailRowButtonStyle())
            Button("Another Row") {}
                .buttonStyle(DetailRowButtonStyle())
        }
        .padding()
        .frame(width: 360, alignment: .leading)
    }
#endif
