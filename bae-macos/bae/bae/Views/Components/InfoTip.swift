import BaeKit
import SwiftUI

/// A "?" icon that shows an info popover while the cursor is over the icon or
/// the popover.
struct InfoTip: View {
    let text: LocalizedStringKey
    var learnMoreURL: URL?
    var width: CGFloat = 260
    var arrowEdge: Edge = .top

    var body: some View {
        Image(systemName: "questionmark.circle")
            .themeIcon(.small)
            .foregroundStyle(.tertiary)
            .hoverPopover(arrowEdge: arrowEdge) {
                VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                    Text(text)
                        .themeText(.body)
                    if let url = learnMoreURL {
                        Link("Learn more", destination: url)
                            .themeText(.body)
                    }
                }
                .padding(ThemeSpace.group)
                .frame(width: width)
            }
    }
}
