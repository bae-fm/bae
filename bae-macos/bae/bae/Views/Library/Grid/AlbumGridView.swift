import BaeKit
import SwiftUI
import os.log

private let albumGridLogger = Logger.bae("AlbumGridView")
private let albumCardSize: CGFloat = 200
private let gridSpacing = ThemeSpace.page

struct AlbumGridView<ExpansionContent: View>: View {
    @Environment(UiStore.self)
    private var uiStore
    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(Library.self)
    private var library
    let list: AlbumList
    /// The active sort, which `revealAlbum` needs to resolve an album's index.
    let sortCriteria: [BridgeSortCriterion]
    /// Span the window instead of the capped column; it feeds the column
    /// count, so the cap sits inside the ScrollView.
    let fullWidth: Bool
    /// The multi-selection, which modifier clicks and Esc change.
    let selection: AlbumGridSelection
    /// Bulk actions, each given the target album ids in grid order.
    let onPlay: ([String]) -> Void
    let onAddToQueue: ([String]) -> Void
    let onAddNext: ([String]) -> Void
    @ViewBuilder
    let expansionContent: (_ albumId: String) -> ExpansionContent

    /// Focused on a selection click, so Esc works right after it.
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
                    LazyVStack(alignment: .leading, spacing: ThemeSpace.page) {
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
                                            AlbumCardPlaceholder(
                                                size: cardWidth
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
                    // A click on the empty background clears the selection.
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
    /// Scrolls to `albumId` by asking core for its index and loading its page;
    /// the scroll comes last, so a cancelled reveal changes nothing.
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

            // Load the target's page so its row exists to scroll to.
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

    /// Cmd toggles the album in the selection, shift extends the range, and a
    /// plain click clears the selection and toggles the album's detail.
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

    /// The card's menu, acting on the whole selection when the card is in it.
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

    /// The card's drag payload: the whole selection when the card is in it.
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
        /// The seeded store, which the `#Preview` root also injects so the
        /// preview audit resolves the detail view's environment there.
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
