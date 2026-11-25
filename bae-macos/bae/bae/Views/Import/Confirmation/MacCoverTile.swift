import BaeKit
import SwiftUI

/// One image in the cover gallery: the image over its name, ringed while the
/// preview shows it. A click shows it in the preview and a double click opens
/// it in the lightbox.
struct MacCoverTile: View {
    let item: CoverItem
    let selected: Bool
    let onFocus: () -> Void
    let onOpenLightbox: () -> Void
    /// Whether the lightbox is drawn over the gallery: the tile, disabled
    /// then, draws as it is under the lightbox's backdrop.
    let isCovered: Bool

    var body: some View {
        Button(action: onFocus) {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Artwork(
                    content: item.image,
                    contentMode: .fit,
                    pointSize: 180,
                    cornerRadius: nil
                )
                // The picture's own corners, which Artwork leaves to the
                // view around it when it fits its picture.
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                .padding(ThemeSpace.related + ThemeSpace.line)
                .frame(height: 138)
                .frame(maxWidth: .infinity)
                .background(Theme.hover)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                .overlay {
                    RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                        .strokeBorder(
                            selected ? Theme.accent : .clear,
                            lineWidth: 2
                        )
                }
                Text(verbatim: item.label)
                    .themeText(.rowTitle)
                    .lineLimit(1)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.accentPlain(dimsWhenDisabled: !isCovered))
        .help(item.label)
        .accessibilityLabel(item.label)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .simultaneousGesture(
            TapGesture(count: 2)
                .onEnded {
                    onFocus()
                    onOpenLightbox()
                }
        )
    }
}
