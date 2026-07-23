import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("Queue")

/// Range fetched around an unloaded context row as the queue scrolls, mirroring
/// the page size `PlaybackStore.loadUpcomingRange`'s bridge call uses.
private let queueUpcomingLoadBatchSize = 100

// periphery:ignore
/// Row load identity: the store's queue revision (so a bump restarts every
/// visible row's load task) plus the row's absolute index. The manual lane,
/// always fully loaded, passes a constant epoch and a `nil` load hook so its
/// rows never fire a load.
private struct QueueRowLoadID: Hashable {
    let epoch: UInt64
    let index: Int
}

/// One queue lane's row-addressing and on-demand loading: how many rows it
/// spans, how to resolve the item at an absolute index (`nil` when not yet
/// loaded), the load epoch that restarts a row's fetch task when the queue
/// revision bumps, and the range fetch for unloaded rows — `nil` for the
/// always-fully-loaded manual lane, which never pages. The context lane fills
/// these from its windowed store; the manual lane from its in-memory array.
struct QueueLane {
    let count: Int
    let itemAt: (Int) -> QueueItem?
    let loadEpoch: UInt64
    let loadRange: ((_ offset: Int, _ limit: Int) async -> Void)?
}

/// The "Up Next" rows — shared by the queue sheet and the expanded player's
/// embedded queue so the index conventions can't drift. Addressed by absolute
/// index via `itemAt`, not a concrete array, so the context lane can be a
/// library-scaled, partly-loaded tail: an unloaded index renders a placeholder
/// and `.task(id:)` fetches the range around it via `loadRange` (`nil` for the
/// always-fully-loaded manual lane). Tapping a loaded row always skips to it and
/// runs `onSkipped`; swiping a loaded row deletes it; `.onMove` reorders.
///
/// Reorder handles are always visible in both surfaces — no `EditButton`, no
/// edit-mode toggle — matching Apple Music's Up Next. That needs
/// `EditMode.active` scoped to just this `ForEach` (`.onMove` is inert
/// without it), which would normally also swap every row's trailing swipe
/// gesture for the edit-mode delete (minus) control; using `.swipeActions`
/// instead of `.onDelete` keeps the delete affordance a real swipe regardless
/// of edit mode (and, since it's attached only when `item` is loaded, an
/// unloaded row is simply not swipeable rather than swiping and silently
/// no-op'ing). The now-playing row and the section headers sit outside this
/// `ForEach`, so the constant edit mode never reaches them.
///
/// A reorder whose source or target row isn't loaded is dropped with a
/// warning rather than guessed — reordering only ever targets a rendered
/// (hence loaded) row in practice, so this only ever excludes a brief loading
/// flash, not a real position.
@MainActor
@ViewBuilder
func upNextRows(
    lane: QueueLane,
    queue: Queue,
    onSkipped: @escaping () -> Void
) -> some View {
    ForEach(0..<lane.count, id: \.self) { index in
        let item = lane.itemAt(index)
        Group {
            if let item {
                Button {
                    queue.skipToEntry(item.entryId)
                    onSkipped()
                } label: {
                    QueueRow(item: item)
                }
                .buttonStyle(.plain)
                .swipeActions(edge: .trailing) {
                    Button(role: .destructive) {
                        queue.removeEntry(item.entryId)
                    } label: {
                        Label("Remove", systemImage: "trash")
                    }
                }
            }
            else {
                QueueRowPlaceholder()
            }
        }
        .task(id: QueueRowLoadID(epoch: lane.loadEpoch, index: index)) {
            guard item == nil, let loadRange = lane.loadRange else {
                return
            }
            let first = max(0, index - queueUpcomingLoadBatchSize / 2)
            let end = min(first + queueUpcomingLoadBatchSize, lane.count)
            await loadRange(first, end - first)
        }
    }
    .onMove { source, destination in
        reorderQueueEntry(in: lane, queue: queue, from: source, to: destination)
    }
    .environment(\.editMode, .constant(.active))
}

/// Apply a SwiftUI `onMove` to the queue by entry id. A move whose source or
/// target row isn't loaded is dropped with a warning rather than guessed —
/// reorder only ever targets a rendered (hence loaded) row in practice, so this
/// only ever excludes a brief loading flash, not a real position.
@MainActor
func reorderQueueEntry(
    in lane: QueueLane,
    queue: Queue,
    from source: IndexSet,
    to destination: Int
) {
    guard let from = source.first else {
        // SwiftUI always hands onMove a non-empty source; an empty set is a
        // framework anomaly, so surface it rather than silently dropping it.
        logger.warning("onMove fired with an empty source index set")
        return
    }
    guard let fromItem = lane.itemAt(from) else {
        logger.warning("onMove source at index \(from) is not loaded")
        return
    }
    // SwiftUI's destination is a gap index in the original array (item still
    // present): the moved entry lands before the item currently at that gap,
    // or at the end when the gap is past the last row.
    let beforeEntryId: String?
    if destination < lane.count {
        guard let target = lane.itemAt(destination) else {
            logger.warning("onMove destination at index \(destination) is not loaded")
            return
        }
        beforeEntryId = target.entryId
    }
    else {
        beforeEntryId = nil
    }
    queue.reorderEntry(fromItem.entryId, beforeEntryId)
}
