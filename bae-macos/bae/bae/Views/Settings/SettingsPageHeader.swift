import BaeKit
import SwiftUI

/// The top of a settings page that is not the tab's first: the page before it,
/// to go back to, and this page's title.
struct SettingsPageHeader: View {
    let title: String
    let backTitle: String
    let onBack: () -> Void

    var body: some View {
        ZStack {
            Text(verbatim: title)
                .themeText(.heading)
                .lineLimit(1)
            HStack {
                Button(action: onBack) {
                    HStack(spacing: ThemeSpace.line) {
                        Image(systemName: "chevron.left")
                        Text(verbatim: backTitle)
                    }
                }
                .buttonStyle(.borderless)
                Spacer()
            }
        }
        .padding(.horizontal)
        .padding(.vertical, ThemeSpace.related)
    }
}
