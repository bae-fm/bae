import BaeKit
import SwiftUI
import os.log

private let albumGridLogger = Logger.bae("AlbumGridView")
private let albumCardSize: CGFloat = 200
private let gridSpacing: CGFloat = 30

struct AlbumGridView<ExpansionContent: View>: View {
    @Environment(UiStore.self)
    private var uiStore
    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(Library.self)
    private var library
    let list: AlbumList
    /// The active sort, owned by `LibraryView`. Read-only here: the grid needs
    /// it to resolve an album's index for `revealAlbum`; the sort *controls*
    /// live in `LibraryView`'s pinned header.
    let sortCriteria: [BridgeSortCriterion]
    /// Span the window instead of centering in the shared capped column
    /// (`Config.libraryFullWidth`). Feeds the column-count math, so the cap
    /// must be inside the ScrollView rather than wrapped around it.
    let fullWidth: Bool
    /// Multi-selection state, owned by `LibraryView`. The grid reads it to render
    /// the selection tint and build bulk-action targets, and mutates it on
    /// modifier clicks / Esc / cmd-A.
    let selection: AlbumGridSelection
    /// Bulk-action closures. Each takes the album ids to act on, in visible grid
    /// order — one album for a plain card, the whole selection for a selected one.
    let onPlay: ([String]) -> Void
    let onAddToQueue: ([String]) -> Void
    let onAddNext: ([String]) -> Void
    @ViewBuilder
    let expansionContent: (_ albumId: String) -> ExpansionContent

    /// Focus lands on the grid the moment a selection interaction happens, so Esc
    /// (clear) and cmd-A (select all loaded) work immediately after a click.
    @FocusState
    private var gridFocused: Bool

    var body: some View {
        GeometryReader { geometry in
            let effectiveWidth =
                (fullWidth
                    ? geometry.size.width
                    : min(geometry.size.width, LibraryContentContainer.maxWidth))
                - LibraryContentContainer.horizontalPadding * 2
            let columnCount = max(
                1,
                Int(
                    floor(
                        (effectiveWidth + gridSpacing)
                            / (albumCardSize + gridSpacing)
                    )
                )
            )
            let cardWidth =
                (effectiveWidth - CGFloat(columnCount - 1) * gridSpacing)
                / CGFloat(columnCount)
            let rowCount = list.rowCount(columnCount: columnCount)

            ScrollViewReader { scrollProxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 34) {
                        ForEach(0..<rowCount, id: \.self) { rowIndex in
                            HStack(spacing: gridSpacing) {
                                ForEach(0..<columnCount, id: \.self) {
                                    col in
                                    let albumIndex =
                                        rowIndex * columnCount + col
                                    if albumIndex < list.totalCount {
                                        if let id = list.idAt(albumIndex),
                                            let summary =
                                                libraryStore.albumSummaries[
                                                    id
                                                ]
                                        {
                                            AlbumCardView(
                                                title: summary.title,
                                                artistNames: summary
                                                    .artistNames,
                                                year: summary.year,
                                                cover: summary.cover,
                                                isExpanded: uiStore
                                                    .selectedAlbumId
                                                    == summary.id,
                                                isSelected:
                                                    selection
                                                    .contains(summary.id),
                                                size: cardWidth,
                                                menu: cardMenu(
                                                    for: summary.id
                                                ),
                                            )
                                            .id(summary.id)
                                            .frame(width: cardWidth)
                                            .draggable(
                                                dragPayload(for: summary.id)
                                            )
                                            .onTapGesture {
                                                handleTap(on: summary.id)
                                            }
                                        }
                                        else {
                                            Color.clear
                                                .aspectRatio(
                                                    1,
                                                    contentMode: .fit
                                                )
                                                .frame(width: cardWidth)
                                        }
                                    }
                                }
                                if list.totalCount > 0 {
                                    let albumsInRow = min(
                                        columnCount,
                                        list.totalCount - rowIndex
                                            * columnCount
                                    )
                                    if albumsInRow < columnCount {
                                        Spacer()
                                    }
                                }
                            }
                            .id(rowIndex)
                            .task(
                                id: RowLoadID(
                                    epoch: list.loadEpoch,
                                    index: rowIndex
                                )
                            ) {
                                await list.loadPage(
                                    containing: rowIndex * columnCount
                                )
                            }
                            AlbumExpansionSlot(
                                selectedId: selectedAlbumId(
                                    rowIndex: rowIndex,
                                    columnCount: columnCount
                                ),
                                expansionContent: expansionContent
                            )
                        }
                    }
                    .padding(
                        .horizontal,
                        LibraryContentContainer.horizontalPadding
                    )
                    .padding(.bottom)
                    .libraryContentContainer(fullWidth: fullWidth)
                    // A click on the empty grid background (not on a card, whose
                    // own tap wins) clears the multi-selection.
                    .contentShape(Rectangle())
                    .onTapGesture {
                        if !selection.isEmpty {
                            selection.clear()
                        }
                    }
                }
                .reportsHeaderScroll(id: "albumGrid")
                .focusable()
                .focusEffectDisabled()
                .focused($gridFocused)
                .onKeyPress(.escape) {
                    guard !selection.isEmpty else {
                        return .ignored
                    }
                    selection.clear()
                    return .handled
                }
                .onKeyPress(keys: ["a"]) { keyPress in
                    guard keyPress.modifiers.contains(.command) else {
                        return .ignored
                    }
                    selection.selectAll(list.allLoadedIds)
                    return .handled
                }
                .task(id: uiStore.pendingAlbumReveal?.seq) {
                    guard let reveal = uiStore.pendingAlbumReveal else {
                        return
                    }
                    await revealAlbum(
                        reveal.albumId,
                        columnCount: columnCount,
                        scrollProxy: scrollProxy
                    )
                    uiStore.consumeAlbumReveal(seq: reveal.seq)
                }
            }
        }
    }
}

extension AlbumGridView {
    /// Scroll the grid to `albumId`, resolving its row deterministically.
    ///
    /// The album's page may never have been fetched, so `list.position(of:)`
    /// can't be trusted. Ask the core for the album's index under the current
    /// sort (off the main actor), load the page that contains it, then scroll.
    /// Each async stage checks `Task.isCancelled`, and the scroll — the only
    /// durable effect — commits last, so a cancel (a newer reveal, or the grid
    /// disappearing) before that point changes nothing — the pending reveal
    /// stays set and a later remount retries it. SwiftUI drives the
    /// cancellation by keying the calling `.task` on `pendingAlbumReveal.seq`;
    /// the caller consumes (clears) the reveal only after this returns, so a
    /// cancelled attempt never marks an unfinished reveal as done.
    private func revealAlbum(
        _ albumId: String,
        columnCount: Int,
        scrollProxy: ScrollViewProxy
    ) async {
        let getAlbumIndex = library.getAlbumIndex
        let sort = sortCriteria
        do {
            let resolved = try await getAlbumIndex(sort, albumId)
            if Task.isCancelled {
                return
            }
            guard let index = resolved.map(Int.init) else {
                albumGridLogger.warning(
                    "No index for album \(albumId) under the current sort; skipping reveal"
                )
                return
            }

            // Load the page that holds the target so its row exists to scroll to.
            await list.loadPage(containing: index)
            if Task.isCancelled {
                return
            }

            let rowIndex = index / columnCount
            withAnimation(.easeInOut(duration: 0.3)) {
                scrollProxy.scrollTo(rowIndex, anchor: .top)
            }
        }
        catch {
            uiStore.showError(error)
        }
    }

    /// Dispatch a click by its modifiers (read at tap time): cmd toggles the
    /// clicked album in the selection, shift extends the range from the anchor,
    /// and a plain click clears the multi-selection and toggles the detail
    /// expansion (the pre-existing behavior). Modifier clicks focus the grid so
    /// Esc / cmd-A work immediately.
    private func handleTap(on albumId: String) {
        let modifiers = NSEvent.modifierFlags
        if modifiers.contains(.command) {
            selection.toggle(albumId)
            gridFocused = true
        }
        else if modifiers.contains(.shift) {
            selection.extendRange(
                to: albumId,
                position: { list.position(of: $0) },
                idAt: { list.idAt($0) }
            )
            gridFocused = true
        }
        else {
            selection.clear()
            withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) {
                uiStore.selectAlbum(
                    uiStore.selectedAlbumId == albumId ? nil : albumId
                )
            }
        }
    }

    /// The bulk-action menu for a card, bound to the album ids the click targets:
    /// the whole selection (visible order) when the card is part of a
    /// multi-selection, else just this album.
    private func cardMenu(for albumId: String) -> AlbumCardMenu {
        let targets = selection.orderedTargets(
            for: albumId,
            position: { list.position(of: $0) }
        )
        return AlbumCardMenu(
            targetCount: targets.count,
            onPlay: { onPlay(targets) },
            onAddToQueue: { onAddToQueue(targets) },
            onAddNext: { onAddNext(targets) }
        )
    }

    /// The drag payload for a card: the whole ordered selection when the card is
    /// part of it, else just this album id.
    private func dragPayload(for albumId: String) -> String {
        AlbumDragPayload.encode(
            selection.orderedTargets(
                for: albumId,
                position: { list.position(of: $0) }
            )
        )
    }

    private func selectedAlbumId(rowIndex: Int, columnCount: Int) -> String? {
        guard let selectedId = uiStore.selectedAlbumId else {
            return nil
        }
        let rowContainsSelection = (0..<columnCount)
            .contains { col in
                let index = rowIndex * columnCount + col
                return index < list.totalCount && list.idAt(index) == selectedId
            }
        return rowContainsSelection ? selectedId : nil
    }
}

#if DEBUG
    // MARK: - Previews

    private struct GridPreview: View {
        let width: CGFloat
        let height: CGFloat
        /// The seeded store the grid interns its list into. Shared with the
        /// `#Preview` root, which injects the same instance via
        /// `albumDetailPreviewEnvironment` so the audit resolves the
        /// `AlbumDetailView` chain's environment from one place.
        let store: LibraryStore
        private let sortCriteria: [BridgeSortCriterion] = [
            BridgeSortCriterion(field: .dateAdded, direction: .descending)
        ]

        var body: some View {
            let list = AlbumList.preview(
                albums: PreviewData.albums,
                store: store
            )
            AlbumGridView(
                list: list,
                sortCriteria: sortCriteria,
                fullWidth: false,
                selection: AlbumGridSelection(),
                onPlay: { _ in },
                onAddToQueue: { _ in },
                onAddNext: { _ in },
            ) { albumId in
                AlbumDetailView(albumId: albumId)
            }
            .frame(width: width, height: height)
            // The production grid is backdropped by LibraryView's page
            // gradient; previews stand in with the flat base color.
            .background(Theme.background)
        }
    }

    #Preview("Grid \u{2014} Wide") {
        let store = PreviewData.seededLibraryStore()
        GridPreview(width: 1100, height: 700, store: store)
            .albumDetailPreviewEnvironment(store: store)
    }

    #Preview("Grid \u{2014} Medium") {
        let store = PreviewData.seededLibraryStore()
        GridPreview(width: 700, height: 600, store: store)
            .albumDetailPreviewEnvironment(store: store)
    }

    #Preview("Grid \u{2014} Narrow") {
        let store = PreviewData.seededLibraryStore()
        GridPreview(width: 400, height: 600, store: store)
            .albumDetailPreviewEnvironment(store: store)
    }
#endif
