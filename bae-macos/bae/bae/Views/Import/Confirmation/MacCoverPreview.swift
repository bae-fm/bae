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
                        cornerRadius: nil
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
                    Button {
                        onFocus(focus.neighbors?.previous)
                    } label: {
                        Image(systemName: "chevron.backward")
                    }
                    .accessibilityLabel(Text("Previous image"))
                    Text("\(Int(focus.position) + 1) of \(content.total)")
                        .monospacedDigit()
                        .foregroundStyle(Theme.textSecondary)
                    Button {
                        onFocus(focus.neighbors?.next)
                    } label: {
                        Image(systemName: "chevron.forward")
                    }
                    .accessibilityLabel(Text("Next image"))
                }
                .buttonStyle(.accentBorderless)
                .disabled(focus.neighbors == nil || isSaving)
            }
        }
        else {
            MacContentUnavailable(title: "No cover art available")
        }
    }
}
