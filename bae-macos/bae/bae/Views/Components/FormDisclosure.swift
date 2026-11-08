import BaeKit
import SwiftUI

/// A section of a grouped form that folds part of one group away: its own
/// box, the chevron leading inside the label's row, pointing right and
/// turning down when open, and the rows it shows indented to the label.
struct FormDisclosure<Label: View, Content: View>: View {
    @Binding
    var isExpanded: Bool
    @ViewBuilder
    let content: Content
    @ViewBuilder
    let label: Label

    var body: some View {
        Section {
            Button {
                isExpanded.toggle()
            } label: {
                HStack(spacing: 0) {
                    Image(systemName: "chevron.right")
                        .themeIcon(.small)
                        .foregroundStyle(.secondary)
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                        .frame(width: ThemeSpace.hang, alignment: .leading)
                    label
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityAddTraits(.isHeader)
            if isExpanded {
                Group { content }
                    .padding(.leading, ThemeSpace.hang)
            }
        }
    }
}
