import BaeKit
import SwiftUI

/// The list row for a queued track, with its playback and removal actions.
struct MacQueueRow: View {
    let item: BridgeQueueEntry
    let isHovered: Bool
    let onHoverChanged: (Bool) -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void

    static let artworkSize: CGFloat = 40
    static let horizontalInset = MacListRow.horizontalInset
    static let verticalInset = MacListRow.verticalInset

    var body: some View {
        MacListRow(
            coverSize: Self.artworkSize,
            cover: item.display.coverImage,
            title: item.display.title,
            subtitle: item.display.artistLine.text,
            duration: item.durationLabel,
            hover: isHovered,
            removal: MacListRow.Removal(label: "Remove from queue") {
                onRemove(item.id)
            }
        )
        .onHover(perform: onHoverChanged)
        .onTapGesture(count: 2) {
            onSkipTo(item.id)
        }
        .macContextMenu {
            MacMenuItem("Remove from queue") {
                onRemove(item.id)
            }
        }
    }
}
