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

        // Load string IDs from the pasteboard.
        let providers = info.itemProviders(for: [UTType.plainText])
        guard !providers.isEmpty else {
            dropInsertIndex = nil
            return false
        }

        let insertAt = targetIndex
        let serialQueue = DispatchQueue(label: "bae.queue-drop-collect")
        nonisolated(unsafe) var collectedIds: [String] = []
        let group = DispatchGroup()

        for provider in providers {
            group.enter()
            provider.loadItem(forTypeIdentifier: UTType.plainText.identifier) {
                item,
                _ in
                if let data = item as? Data,
                    let str = String(data: data, encoding: .utf8)
                {
                    serialQueue.sync { collectedIds.append(str) }
                }
                else if let str = item as? String {
                    serialQueue.sync { collectedIds.append(str) }
                }
                group.leave()
            }
        }

        group.notify(queue: .main) {
            // A dragged album card may carry several ids joined by a newline (a
            // multi-selection drag); split each back out before resolving.
            let ids = collectedIds.flatMap(AlbumDragPayload.decode)
            if !ids.isEmpty {
                onInsertTracks(ids, insertAt)
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
