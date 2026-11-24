import BaeKit
import SwiftUI

/// A modal's header: the title (22 point with `large`), a second line under
/// it where the modal has one, and `buttons` at the trailing edge, with a
/// divider under them.
struct MacModalHeader<Buttons: View>: View {
    let title: LocalizedStringKey
    var subtitle: String?
    var large = false
    @ViewBuilder
    var buttons: () -> Buttons

    var body: some View {
        VStack(spacing: 0) {
            content
            MacHairline()
        }
    }

    private var content: some View {
        HStack(spacing: ThemeSpace.related) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                // VoiceOver reads the title and the line under it before the
                // buttons, which sit level with the title's middle.
                Text(title)
                    .themeText(large ? .title : .heading)
                    .accessibilitySortPriority(1)
                if let subtitle {
                    Text(subtitle)
                        .themeText(.detail)
                        .foregroundStyle(Theme.textSecondary)
                        .lineLimit(1)
                        .accessibilitySortPriority(1)
                }
            }
            Spacer()
            buttons()
        }
        // The sort priorities above order the header's own elements. Without
        // this group they order the title and line against every element in
        // the window, and VoiceOver reads them before the page under the
        // modal.
        .accessibilityElement(children: .contain)
        .padding(.vertical, ThemeSpace.group + ThemeSpace.line)
        .padding(.horizontal, ThemeSpace.edge)
    }
}
