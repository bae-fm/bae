import BaeKit
import SwiftUI

/// One pane of the settings window: its bold title at the top left, the
/// pane's content under it.
struct SettingsPane<Content: View>: View {
    let tab: SettingsTab
    @ViewBuilder
    let content: Content

    init(_ tab: SettingsTab, @ViewBuilder content: () -> Content) {
        self.tab = tab
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SettingsPaneHeading { Text(tab.title) }
            content
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: .top
                )
        }
    }
}

/// The measures of a pane's edges: the boards' 18 pt over the title and 20 pt
/// at the sides.
enum SettingsPaneMetrics {
    static let sideInset: CGFloat = 20
    static let topInset: CGFloat = 18
}

/// The pane's heading: bold, at the top left, with the room under it the first
/// box starts after.
struct SettingsPaneHeading<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            content
                .themeText(.heading)
                .lineLimit(1)
        }
        .padding(.horizontal, SettingsPaneMetrics.sideInset)
        .padding(.top, SettingsPaneMetrics.topInset)
        .padding(.bottom, ThemeSpace.group)
    }
}
