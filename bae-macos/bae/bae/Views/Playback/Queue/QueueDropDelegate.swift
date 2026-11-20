import BaeKit
import Foundation
import SwiftUI
import UniformTypeIdentifiers

/// External album-card drops into the manual lane — the one drag interaction
/// still on the OS drag machinery, because it genuinely crosses views (the
/// library grid to this pane). Internal reorders and cross-lane enqueues are
/// gesture-driven in `QueueSection` and never reach this delegate.
struct QueueDropDelegate: DropDelegate {
    /// The insertion gap this target addresses (`targetIndex == count` is the
    /// trailing append zone, past the last row).
    let targetIndex: Int
    /// Whether external track drops are accepted into this lane. The manual
    /// lane inserts them; the context lane (the release's own order) refuses.
    let acceptsExternalDrops: Bool
    @Binding
    var dropInsertIndex: Int?
    let onInsertTracks: ([String], Int) -> Void
    /// Shows that a drop's items failed to load.
    let onDropFailed: @MainActor (DisplayError) -> Void

    func dropEntered(info _: DropInfo) {
        if acceptsExternalDrops {
            dropInsertIndex = targetIndex
        }
    }

    func dropUpdated(info _: DropInfo) -> DropProposal? {
        DropProposal(operation: acceptsExternalDrops ? .copy : .forbidden)
    }

    func performDrop(info: DropInfo) -> Bool {
        guard acceptsExternalDrops else {
            dropInsertIndex = nil
            return false
        }

        let insertAt = targetIndex
        info.loadPlainText(failed: onDropFailed) { collectedIds in
            // Each dropped string as it came: an album drag's payload, which
            // the receiver has core order, or a track's id.
            if !collectedIds.isEmpty {
                onInsertTracks(collectedIds, insertAt)
            }
        }

        dropInsertIndex = nil
        return true
    }

    func dropExited(info _: DropInfo) {
        dropInsertIndex = nil
    }

    func validateDrop(info: DropInfo) -> Bool {
        acceptsExternalDrops
            && info.hasItemsConforming(to: [UTType.plainText])
    }
}
