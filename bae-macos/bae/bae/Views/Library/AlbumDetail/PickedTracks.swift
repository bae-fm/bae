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

    /// Pick `range`, the tracks core read from the anchor to a shift-clicked
    /// row. The anchor stays.
    func extend(over range: [String]) {
        ids.formUnion(range)
    }

    func clear() {
        ids = []
        anchorId = nil
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

/// Picking on a track list: the picked set, what a shift-click on a row
/// picks, and what a menu on it does, each action given the picked ids in no
/// order.
struct TrackPicking {
    let picked: PickedTracks
    /// Pick the rows from the anchor to the row `targetId`, as core reads
    /// them.
    let onExtend: (_ targetId: String) -> Void
    let onPlay: ([String]) -> Void
    let onAddNext: ([String]) -> Void
    let onAddToQueue: ([String]) -> Void
}
