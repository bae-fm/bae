import BaeKit
import SwiftUI

/// A modal's header: the title (22 point with `large`), a second line under
/// it where the modal has one, and `buttons` at the trailing edge.
struct MacModalHeader<Buttons: View>: View {
    let title: LocalizedStringKey
    var subtitle: String?
    var large = false
    @ViewBuilder
    var buttons: () -> Buttons

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(title)
                    .themeText(large ? .title : .heading)
                if let subtitle {
                    Text(subtitle)
                        .themeText(.detail)
                        .foregroundStyle(Theme.textSecondary)
                        .lineLimit(1)
                }
            }
            Spacer()
            buttons()
        }
        .padding(.vertical, ThemeSpace.group + ThemeSpace.line)
        .padding(.horizontal, ThemeSpace.edge)
    }
}
