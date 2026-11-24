import BaeKit
import SwiftUI

/// A dashed box with a word in it, drawn where a component's preview draws
/// `bae-preview-slot` for content the component does not own. It fills the
/// width it is given, as the preview's slot does.
struct PreviewSlot: View {
    let label: String

    init(_ label: String) { self.label = label }

    var body: some View {
        Text(verbatim: label)
            .foregroundStyle(Theme.textSecondary)
            .padding(ThemeSpace.group)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay {
                RoundedRectangle(cornerRadius: ThemeRadius.card)
                    .strokeBorder(
                        Theme.hairlineStrong,
                        style: StrokeStyle(lineWidth: 1, dash: [4, 3])
                    )
            }
    }
}
