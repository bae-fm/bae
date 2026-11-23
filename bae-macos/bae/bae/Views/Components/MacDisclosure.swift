import BaeKit
import SwiftUI

/// A section of a grouped form that folds part of one group away: its own
/// box, the chevron centred in the leading column so it has room to turn,
/// pointing forward and turning down when open. The label and the rows it shows
/// start on that column's edge, as `MacItem`'s text does.
struct MacDisclosure<Label: View, Content: View>: View {
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
                MacItem {
                    DisclosureChevron(isExpanded: isExpanded)
                        .themeIcon(.small)
                        .foregroundStyle(Theme.textSecondary)
                } content: {
                    label
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.accentPlain)
            .accessibilityAddTraits(.isHeader)
            if isExpanded {
                Group { content }
                    .padding(.leading, ThemeSize.lead + ThemeSpace.related)
            }
        }
    }
}
