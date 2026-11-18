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
    /// The card a shift-click extends from: its album, at the list position
    /// it was clicked at, in its artist's group when the grid groups by
    /// artist.
    struct Anchor {
        let albumId: String
        let position: Int
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

    /// Set the album's membership and anchor at the clicked card, at list
    /// position `position`.
    func setSelected(
        _ id: String,
        selected: Bool,
        position: Int,
        sectionId: String? = nil
    ) {
        if selected {
            selectedIds.insert(id)
        }
        else {
            selectedIds.remove(id)
        }
        anchor = Anchor(albumId: id, position: position, sectionId: sectionId)
        onSelectionChanged(selectedIds)
    }

    /// shift-click on the card of `targetId` at list position `position`:
    /// union the contiguous range of positions between the anchor's and the
    /// target's into the selection. Ids in the range that aren't loaded
    /// (`idAt` returns nil) are skipped. The anchor is unchanged. With no
    /// anchor, the clicked album is selected and anchored.
    func extendRange(
        to targetId: String,
        position: Int,
        sectionId: String? = nil,
        idAt: (Int) -> String?
    ) {
        guard let anchor else {
            setSelected(
                targetId,
                selected: true,
                position: position,
                sectionId: sectionId
            )
            return
        }
        for index in min(
            anchor.position,
            position
        )...max(anchor.position, position) {
            if let id = idAt(index) {
                selectedIds.insert(id)
            }
        }
        onSelectionChanged(selectedIds)
    }

    /// Forget the anchor, keeping the selection: the list it was clicked in
    /// has moved its rows, so its position no longer names its card.
    func dropAnchor() {
        anchor = nil
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
    /// the whole selection; any other card targets just itself. The ids come in
    /// no order: whatever acts on them hands them to core, which orders them as
    /// the grid lists them. Never mutates the selection — merely opening a menu
    /// or starting a drag leaves it as is.
    func targets(for clickedId: String) -> [String] {
        guard selectedIds.contains(clickedId), selectedIds.count > 1 else {
            return [clickedId]
        }
        return Array(selectedIds)
    }
}
