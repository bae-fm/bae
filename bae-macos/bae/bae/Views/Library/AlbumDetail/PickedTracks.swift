import Observation

/// The tracks picked on an album's page: which rows are picked and the row a
/// shift-click extends from. This class holds only logic and uses neither
/// SwiftUI nor the bridge. Cmd-click picks or unpicks a row, shift-click
/// picks the range from the anchor, and a plain click clears the picks. Core
/// decides what the picked set plays, queues or adds, and in what order: the
/// ids go to core in no order.
@Observable
final class PickedTracks {
    private(set) var ids: Set<String> = []
    private(set) var anchorId: String?

    func contains(_ id: String) -> Bool {
        ids.contains(id)
    }

    /// Set the row's membership and make it the anchor.
    func set(_ id: String, picked: Bool) {
        if picked {
            ids.insert(id)
        }
        else {
            ids.remove(id)
        }
        anchorId = id
    }

    /// Pick every row from the anchor to `targetId`, both ends included, in
    /// `order`, the page's row order. Without an anchor on the page, picks
    /// and anchors `targetId` alone. The anchor stays.
    func extend(to targetId: String, in order: [String]) {
        guard let anchorId,
            let anchorIndex = order.firstIndex(of: anchorId),
            let targetIndex = order.firstIndex(of: targetId)
        else {
            set(targetId, picked: true)
            return
        }
        ids.formUnion(
            order[min(anchorIndex, targetIndex)...max(anchorIndex, targetIndex)]
        )
    }

    func clear() {
        ids = []
        anchorId = nil
    }

    /// Drop the picks that are no longer rows of the page, as `rows` lists
    /// them.
    func keep(_ rows: Set<String>) {
        ids.formIntersection(rows)
        if let anchorId, !rows.contains(anchorId) {
            self.anchorId = nil
        }
    }

    /// What a row's menu acts on: the whole set when the row is among
    /// several picked, otherwise the row alone. Opening a menu leaves the
    /// picks as they are.
    func targets(for clickedId: String) -> [String] {
        guard ids.contains(clickedId), ids.count > 1 else {
            return [clickedId]
        }
        return Array(ids)
    }
}

/// Picking on a track list: the picked set and what a menu on it does, each
/// action given the picked ids in no order.
struct TrackPicking {
    let picked: PickedTracks
    let onPlay: ([String]) -> Void
    let onAddNext: ([String]) -> Void
    let onAddToQueue: ([String]) -> Void
}
