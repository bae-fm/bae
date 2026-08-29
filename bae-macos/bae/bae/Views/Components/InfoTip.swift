import BaeKit
import SwiftUI

/// A "?" icon that shows an info popover on hover.
/// The popover stays open while the cursor is over either the icon or the popover itself.
struct InfoTip: View {
    let text: LocalizedStringKey
    var learnMoreURL: URL?
    var width: CGFloat = 260
    var arrowEdge: Edge = .top

    var body: some View {
        Image(systemName: "questionmark.circle")
            .font(.callout)
            .foregroundStyle(.tertiary)
            .hoverPopover(arrowEdge: arrowEdge) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(text)
                        .font(.callout)
                    if let url = learnMoreURL {
                        Link("Learn more", destination: url)
                            .font(.callout)
                    }
                }
                .padding(10)
                .frame(width: width)
                .popoverEntrance(anchor: entranceAnchor)
                .background { PopoverBehavior() }
            }
    }

    /// The popover's visual anchor: the edge its arrow sits on, which is the
    /// side facing the "?" icon — opposite the `arrowEdge` the icon anchors.
    private var entranceAnchor: UnitPoint {
        switch arrowEdge {
        case .top: .bottom
        case .bottom: .top
        case .leading: .trailing
        case .trailing: .leading
        }
    }

}

#if DEBUG
    // The info popover is hover-driven, so a static preview shows the "?" trigger
    // glyph beside its label; hovering it in the live preview opens the popover.
    // Sample copy routes through String values so the extractor never takes
    // preview-only prose into the catalog.
    #Preview("Info Tip") {
        let encryptionTip =
            "Your library is encrypted with a key only this device holds."
        let watchedFolderTip =
            "New rips dropped here are picked up automatically."
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 8) {
                Text(verbatim: "Encryption key")
                InfoTip(text: LocalizedStringKey(encryptionTip))
            }
            HStack(spacing: 8) {
                Text(verbatim: "Watched folder")
                InfoTip(
                    text: LocalizedStringKey(watchedFolderTip),
                    learnMoreURL: URL(string: "https://example.com/docs"),
                    arrowEdge: .trailing
                )
            }
        }
        .padding(28)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
