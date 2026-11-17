import BaeKit
import SwiftUI

/// The import sidebar: the tabs with their counts, the filter row (the filter
/// field, then the controls the caller puts at its end), and the list beneath
/// them. The sidebar draws the values it is given, except the tab counts,
/// which the tab bar reads from `listSlot`; `MacImportSidebarContainer`
/// reads the stores for it.
struct MacImportSidebar<Trailing: View, Content: View>: View {
    @Binding
    var activeTab: BridgeTriageTab
    /// The list whose count holds every tab's count.
    let listSlot: ImportListSlot
    @Binding
    var filterText: String
    /// Reads the count for the filter the field holds now; `nil` while
    /// nothing narrows the list, and while core has not counted under this
    /// filter. The filter field reads it, so a new count redraws only the
    /// field.
    let narrowed: () -> BridgeNarrowedCount?
    /// Whether the list shows the view the person asked for. Until the read
    /// answering it lands, the list holds the previous view's rows: they stay
    /// unshown and out of reach rather than standing in for this view's.
    let answersView: Bool
    @ViewBuilder
    let trailing: () -> Trailing
    @ViewBuilder
    let content: () -> Content

    @FocusState
    private var filterFocused: Bool

    var body: some View {
        ImportSidebarList {
            VStack(spacing: 0) {
                TriageTabBar(activeTab: $activeTab, listSlot: listSlot)
                    .padding(ThemeSpace.related)

                // Separates choosing the tab from filtering it.
                Divider()

                HStack(spacing: ThemeSpace.related) {
                    ImportListFilterField(
                        text: $filterText,
                        focused: $filterFocused,
                        narrowed: narrowed
                    )
                    trailing()
                }
                .padding(.horizontal, ThemeSpace.group)
                .frame(height: ImportFilterBarLayout.rowHeight)
                // A click anywhere on the row puts the caret in the
                // field; the controls at the end keep their own clicks.
                .contentShape(Rectangle())
                .onTapGesture { filterFocused = true }
            }
        } content: {
            content()
                .opacity(answersView ? 1 : 0)
                .allowsHitTesting(answersView)
                .accessibilityHidden(!answersView)
        }
    }
}
