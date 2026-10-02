import Observation

/// Multi-selection state for the album grid: which albums are selected and the
/// anchor a shift-click extends from. Pure logic — no SwiftUI, no bridge — so it
/// is fully unit-testable. Owned by `LibraryBrowseSession` and passed down to
/// the grid.
///
/// "Selection" here is distinct from `UiStore.selectedAlbumId`, which is the one
/// album whose detail expansion is open. Multi-select is modifier-driven
/// (cmd/shift-click); a plain click clears it.
@MainActor
@Observable
final class AlbumGridSelection {
    struct Anchor {
        let albumId: String
        let sectionId: String?
    }

    private(set) var selectedIds: Set<String> = []
    private(set) var anchor: Anchor?
    @ObservationIgnored
    private let onSelectionChanged: @MainActor (Set<String>) -> Void

    init(
        onSelectionChanged: @escaping @MainActor (Set<String>) -> Void = { _ in
        }
    ) {
        self.onSelectionChanged = onSelectionChanged
    }

    var isEmpty: Bool {
        selectedIds.isEmpty
    }

    func contains(_ id: String) -> Bool {
        selectedIds.contains(id)
    }

    /// Set the album's membership and anchor at the clicked appearance.
    func setSelected(_ id: String, selected: Bool, sectionId: String? = nil) {
        if selected {
            selectedIds.insert(id)
        }
        else {
            selectedIds.remove(id)
        }
        anchor = Anchor(albumId: id, sectionId: sectionId)
        onSelectionChanged(selectedIds)
    }

    /// shift-click: union the contiguous range between the anchor and `targetId`
    /// (in grid order) into the selection. Positions come from `position`; ids in
    /// the range that aren't loaded (`idAt` returns nil) are skipped. The anchor is
    /// unchanged. If the anchor no longer resolves — a sort swapped the list and
    /// its segment isn't fetched — the clicked album is selected and anchored.
    func extendRange(
        to targetId: String,
        targetPosition: Int? = nil,
        targetSectionId: String? = nil,
        position: (String) -> Int?,
        idAt: (Int) -> String?
    ) {
        guard let anchor,
            let anchorIndex = position(anchor.albumId),
            let targetIndex = targetPosition ?? position(targetId)
        else {
            setSelected(targetId, selected: true, sectionId: targetSectionId)
            return
        }
        for index in min(
            anchorIndex,
            targetIndex
        )...max(anchorIndex, targetIndex) {
            if let id = idAt(index) {
                selectedIds.insert(id)
            }
        }
        onSelectionChanged(selectedIds)
    }

    func clear() {
        selectedIds = []
        anchor = nil
        onSelectionChanged(selectedIds)
    }

    /// Drop ids that no longer exist (pruned after a delete elsewhere). Clears the
    /// anchor if it was among them.
    func remove(_ ids: [String]) {
        let prior = selectedIds
        selectedIds.subtract(ids)
        if let anchor, ids.contains(anchor.albumId) {
            self.anchor = nil
        }
        if selectedIds != prior {
            onSelectionChanged(selectedIds)
        }
    }

    /// The menu/drag target rule: a card that is part of a multi-selection targets
    /// the whole selection in visible grid order (ids that don't resolve to a
    /// position are dropped); any other card targets just itself. Never mutates
    /// the selection — merely opening a menu or starting a drag leaves it as is.
    func orderedTargets(
        for clickedId: String,
        position: (String) -> Int?
    ) -> [String] {
        guard selectedIds.contains(clickedId), selectedIds.count > 1 else {
            return [clickedId]
        }
        return
            selectedIds
            .compactMap { id in position(id).map { (id, $0) } }
            .sorted { $0.1 < $1.1 }
            .map(\.0)
    }
}
