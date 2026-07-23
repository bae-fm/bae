import BaeKit
import SwiftUI

/// One loaded queue row: cover art (with a hover play overlay), title/album, and
/// a duration that swaps for a remove button on hover. It has no state of its
/// own beyond the remove button's own hover; the row hover is the section's
/// (only one row is ever hovered at a time), flowing in and back out via
/// `isHovered`/`onHoverChanged`.
struct QueueItemRow: View {
    let item: QueueItem
    let isHovered: Bool
    let onHoverChanged: (Bool) -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void

    /// The remove X's own hover, distinct from the row's: it backs the ring
    /// that marks the control as live before any press.
    @State
    private var removeHovered = false

    var body: some View {
        HStack(spacing: 12) {
            artWithHoverOverlay

            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .font(.system(size: 13, weight: .semibold))
                    .lineLimit(1)
                Text(item.albumTitle)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            // The duration swaps for the remove X on row hover, in one fixed
            // slot (opacity toggles, never conditional inclusion) so the row
            // can't resize. Safe now that hover is identity-based: a row
            // sliding under a stationary pointer claims the hover itself, so
            // the X is present exactly when the pointer is on the row.
            ZStack {
                Text(item.durationLabel)
                    .font(
                        .system(size: 11, weight: .semibold).monospacedDigit()
                    )
                    .foregroundStyle(.secondary)
                    .opacity(isHovered ? 0 : 1)
                Button(action: { onRemove(item.id) }) {
                    Image(systemName: "xmark")
                        .font(.caption)
                        .foregroundStyle(
                            removeHovered ? Theme.accent : .secondary
                        )
                        // Same small glyph, comfortable click target.
                        .frame(width: 28, height: 28)
                        .background(
                            RoundedRectangle(cornerRadius: 7)
                                .fill(
                                    Theme.accent.opacity(
                                        removeHovered ? 0.22 : 0
                                    )
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
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        // Hover chrome toggles by fill only — rows stay uniform height, which
        // the drag coordinator's row-pitch math depends on.
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(.white.opacity(isHovered ? 0.06 : 0))
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

    // The hover play overlay stays in the tree and toggles by opacity/hit-testing
    // so revealing it on hover doesn't resize the row and re-lay-out the lane.
    private var artWithHoverOverlay: some View {
        ZStack {
            ImageView(coverImageId: item.coverImageId, pointSize: 44)
                .frame(width: 44, height: 44)
                .clipShape(RoundedRectangle(cornerRadius: 8))

            RoundedRectangle(cornerRadius: 8)
                .fill(.black.opacity(0.5))
                .frame(width: 44, height: 44)
                .opacity(isHovered ? 1 : 0)
            Button(action: { onSkipTo(item.id) }) {
                Image(systemName: "play.fill")
                    .font(.caption)
                    .foregroundColor(.white)
                    // The whole hovered cover is the target, not the glyph.
                    .frame(width: 44, height: 44)
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

    /// Hosts the row's own hover so the "Hovered" variant shows the remove X and
    /// the play overlay the section normally toggles.
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

    // The environment lives on the #Preview root (not inside QueueItemRowPreview's
    // body) so the missing-environment audit, which only reads the preview
    // closure's modifier chain, can see it.
    #Preview("Resting") {
        QueueItemRowPreview(item: PreviewData.queueItems[0], isHovered: false)
            .environment(MediaPaths.stub)
    }

    #Preview("Hovered") {
        QueueItemRowPreview(item: PreviewData.queueItems[1], isHovered: true)
            .environment(MediaPaths.stub)
    }
#endif
