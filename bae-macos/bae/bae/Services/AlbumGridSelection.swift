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
    /// The card a shift-click extends from: its album, in its artist's group
    /// when the grid groups by artist.
    struct Anchor: Equatable {
        let albumId: String
        let sectionId: String?
    }

    private(set) var selectedIds: Set<String> = []
    private(set) var anchor: Anchor?
    /// The read of the albums a shift-click selects, while core reads them.
    /// Any later change to the selection cancels the read, so the selection
    /// never takes in a range that core returns after it changed.
    @ObservationIgnored
    private var rangeRead: Task<Void, Never>?
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

    /// Set the album's membership and anchor at the clicked card, in the
    /// group of the artist `sectionId`.
    func setSelected(
        _ id: String,
        selected: Bool,
        sectionId: String? = nil
    ) {
        rangeRead?.cancel()
        if selected {
            selectedIds.insert(id)
        }
        else {
            selectedIds.remove(id)
        }
        anchor = Anchor(albumId: id, sectionId: sectionId)
        onSelectionChanged(selectedIds)
    }

    /// shift-click on the card of `targetId` in the group of the artist
    /// `sectionId`: add the albums of the cards from the anchor's to the
    /// target's, which `readRange` reads from core, to the selection. The
    /// anchor is unchanged. With no anchor, the clicked album is selected and
    /// anchored. When `readRange` fails, nothing is selected and the error
    /// goes to `onFailure`.
    func extendRange(
        to targetId: String,
        sectionId: String? = nil,
        readRange:
            @escaping @Sendable (_ from: Anchor, _ to: Anchor) async throws
            -> [String],
        onFailure: @escaping @MainActor (any Error) -> Void
    ) {
        guard let anchor else {
            setSelected(targetId, selected: true, sectionId: sectionId)
            return
        }
        let target = Anchor(albumId: targetId, sectionId: sectionId)
        rangeRead?.cancel()
        rangeRead = Task {
            do {
                let range = try await readRange(anchor, target)
                guard !Task.isCancelled else { return }
                selectedIds.formUnion(range)
                onSelectionChanged(selectedIds)
            }
            catch {
                guard !Task.isCancelled else { return }
                onFailure(error)
            }
        }
    }

    func clear() {
        rangeRead?.cancel()
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
