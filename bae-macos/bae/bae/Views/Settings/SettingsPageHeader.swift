import BaeKit
import SwiftUI

/// The top of a settings page that is not the tab's first: a round button back
/// to the page before it, named for it, and this page's title, in the place
/// and type a pane's own title has.
struct SettingsPageHeader: View {
    let title: String
    let backTitle: String
    let onBack: () -> Void

    var body: some View {
        SettingsPaneHeading {
            Button(action: onBack) {
                Image(systemName: "chevron.backward")
                    .themeIcon(.small)
            }
            .buttonStyle(.accentBordered)
            .buttonBorderShape(.circle)
            .controlSize(.small)
            .accessibilityLabel(Text(verbatim: backTitle))
            Text(verbatim: title)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
