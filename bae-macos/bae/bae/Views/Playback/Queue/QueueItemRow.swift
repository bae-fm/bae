import BaeKit
import SwiftUI

/// One loaded queue row: cover art with a hover play overlay, title, artist,
/// album, and a duration that swaps for a remove button on hover. The section
/// owns the row hover and passes it through `isHovered`/`onHoverChanged`.
struct QueueItemRow: View {
    let item: QueueItem
    let isHovered: Bool
    let onHoverChanged: (Bool) -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void

    /// The cover's side, which the row's three lines of text also fill.
    static let artworkSize: CGFloat = 48
    /// The row's inset from its hover fill.
    static let horizontalInset = ThemeSpace.related
    static let verticalInset = ThemeSpace.compact

    /// The remove button's own hover, which fills its background.
    @State
    private var removeHovered = false

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            artWithHoverOverlay

            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(item.title)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                Text(item.artistNames)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Text(item.albumTitle)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            // The duration and the remove button share one slot and toggle by
            // opacity so the row never resizes.
            ZStack {
                Text(item.durationLabel)
                    .themeText(.detail)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .opacity(isHovered ? 0 : 1)
                Button(action: { onRemove(item.id) }) {
                    Image(systemName: "xmark")
                        .themeIcon(.small)
                        .foregroundStyle(
                            removeHovered ? Theme.accent : .secondary
                        )
                        // A larger click target than the glyph.
                        .frame(
                            width: ThemeSize.hitTarget,
                            height: ThemeSize.hitTarget
                        )
                        .background(
                            RoundedRectangle(cornerRadius: ThemeRadius.control)
                                .fill(
                                    removeHovered
                                        ? Theme.accentStrong : Color.clear
                                )
                                .frame(width: 24, height: 24)
                        )
                        .contentShape(Rectangle())
                }
                .buttonStyle(PressableIconButtonStyle())
                .onHover { removeHovered = $0 }
                .help("Remove from queue")
                .opacity(isHovered ? 1 : 0)
                .allowsHitTesting(isHovered)
            }
        }
        .padding(.horizontal, Self.horizontalInset)
        .padding(.vertical, Self.verticalInset)
        // Hover toggles only the fill: the drag coordinator's slot math needs
        // every row at the same height.
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .fill(isHovered ? Theme.hover : Color.clear)
        )
        .contentShape(Rectangle())
        .onHover(perform: onHoverChanged)
        .onTapGesture(count: 2) {
            onSkipTo(item.id)
        }
        .contextMenu {
            Button("Remove from Queue") {
                onRemove(item.id)
            }
        }
    }

    // The play overlay toggles by opacity so showing it doesn't resize the row.
    private var artWithHoverOverlay: some View {
        ZStack {
            ImageView(imageRef: item.coverImage, pointSize: Self.artworkSize)
                .frame(width: Self.artworkSize, height: Self.artworkSize)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))

            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                .fill(Theme.scrim)
                .frame(width: Self.artworkSize, height: Self.artworkSize)
                .opacity(isHovered ? 1 : 0)
            Button(action: { onSkipTo(item.id) }) {
                Image(systemName: "play.fill")
                    .themeIcon(.small)
                    .foregroundStyle(Theme.onFill)
                    // The whole hovered cover is the target, not the glyph.
                    .frame(width: Self.artworkSize, height: Self.artworkSize)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .opacity(isHovered ? 1 : 0)
            .allowsHitTesting(isHovered)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// Holds the hover the section normally owns.
    private struct QueueItemRowPreview: View {
        let item: QueueItem
        @State
        var isHovered: Bool

        var body: some View {
            QueueItemRow(
                item: item,
                isHovered: isHovered,
                onHoverChanged: { isHovered = $0 },
                onSkipTo: { _ in },
                onRemove: { _ in },
            )
            .frame(width: 380)
            .padding()
            .background(Theme.surface)
        }
    }

    // The environment sits on the #Preview root because the missing-environment
    // audit reads only the preview closure.
    #Preview("Resting") {
        QueueItemRowPreview(item: PreviewData.queueItems[0], isHovered: false)
            .environment(ImageStore.stub())
    }

    #Preview("Hovered") {
        QueueItemRowPreview(item: PreviewData.queueItems[1], isHovered: true)
            .environment(ImageStore.stub())
    }
#endif
