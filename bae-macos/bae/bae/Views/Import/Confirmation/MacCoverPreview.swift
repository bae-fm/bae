import BaeKit
import SwiftUI

/// The cover gallery's large preview: the focused cover with its name and
/// source, and the buttons to step to the cover before or after it; an empty
/// notice where no cover is focused. A click on the cover opens the lightbox.
struct MacCoverPreview: View {
    let content: CoverGalleryContent
    let isSaving: Bool
    /// The keyboard focus of the cover button, which the gallery sets.
    var previewFocused: FocusState<Bool>.Binding
    let onOpenLightbox: () -> Void
    /// Focus the cover `key` names, if any.
    let onFocus: (BridgeCoverKey?) -> Void

    var body: some View {
        if let focus = content.focus {
            let cover = CoverItem(gallery: focus.cover)
            VStack(spacing: ThemeSpace.compact) {
                Button(action: onOpenLightbox) {
                    Artwork(
                        content: cover.image,
                        contentMode: .fit,
                        pointSize: 800,
                        cornerRadius: nil,
                        turn: .focused
                    )
                    .frame(width: 300, height: 300)
                    .clipShape(
                        RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                    )
                    .contentShape(Rectangle())
                }
                .buttonStyle(.accentPlain)
                .accessibilityLabel(Text("View in lightbox"))
                .help("View in lightbox")
                .keyboardShortcut(.space, modifiers: [])
                .focusable()
                .focusEffectDisabled()
                .focused(previewFocused)
                .disabled(isSaving)
                Text(verbatim: cover.label)
                    .themeText(.heading)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                Text(verbatim: cover.sourceLabel)
                    .themeText(.detail)
                    .foregroundStyle(Theme.textSecondary)
                HStack(spacing: ThemeSpace.related + ThemeSpace.line) {
                    stepButton(
                        "chevron.backward",
                        label: "Previous image",
                        to: focus.neighbors?.previous
                    )
                    Text("\(Int(focus.position) + 1) of \(content.total)")
                        .monospacedDigit()
                        .foregroundStyle(Theme.textSecondary)
                    stepButton(
                        "chevron.forward",
                        label: "Next image",
                        to: focus.neighbors?.next
                    )
                }
                .buttonStyle(.accentPlain)
                .disabled(focus.neighbors == nil || isSaving)
            }
        }
        else {
            MacContentUnavailable(title: "No cover art available")
        }
    }

    /// A step to the neighbouring cover: the board's 12 by 16 plain button.
    private func stepButton(
        _ symbol: String,
        label: LocalizedStringKey,
        to neighbor: BridgeCoverKey?
    ) -> some View {
        Button {
            onFocus(neighbor)
        } label: {
            Image(systemName: symbol)
                .frame(width: 12, height: 16)
                .contentShape(Rectangle())
        }
        .accessibilityLabel(Text(label))
    }
}
