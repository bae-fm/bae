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
    /// Where the grid is scrolled, held as the slot on top rather than an
    /// offset.
    @State
    private var scrollPosition = ScrollPosition(
        idType: AlbumGridCell.Identity.self
    )

    var body: some View {
        GeometryReader { geometry in
            let metrics = AlbumGridMetrics(
                width: geometry.size.width,
                fullWidth: fullWidth
            )
            ScrollView {
                // One lazy grid over every slot, keyed by album: a new column
                // count moves each card to its new place instead of making
                // new cards in other rows.
                LazyVGrid(
                    columns: metrics.columns,
                    alignment: .leading,
                    spacing: ThemeSpace.page
                ) {
                    ForEach(cells(columnCount: metrics.columnCount)) { cell in
                        slot(cell, metrics: metrics)
                    }
                }
                .scrollTargetLayout()
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
            .scrollPosition($scrollPosition, anchor: .top)
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
                await revealAlbum(reveal.albumId)
                uiStore.consumeAlbumReveal(seq: reveal.seq)
            }
        }
    }
}

/// The grid's columns at one available width.
private struct AlbumGridMetrics {
    /// The width a row spans: every column and the gaps between them.
    let rowWidth: CGFloat
    let columnCount: Int
    let cardWidth: CGFloat

    init(width: CGFloat, fullWidth: Bool) {
        rowWidth = max(
            0,
            (fullWidth ? width : min(width, LibraryContentContainer.maxWidth))
                - LibraryContentContainer.horizontalPadding * 2
        )
        columnCount = max(
            1,
            Int(floor((rowWidth + gridSpacing) / (albumCardSize + gridSpacing)))
        )
        cardWidth = max(
            0,
            (rowWidth - CGFloat(columnCount - 1) * gridSpacing)
                / CGFloat(columnCount)
        )
    }

    var columns: [GridItem] {
        Array(
            repeating: GridItem(
                .fixed(cardWidth),
                spacing: gridSpacing,
                alignment: .topLeading
            ),
            count: columnCount
        )
    }
}

extension AlbumGridView {
    private func cells(columnCount: Int) -> AlbumGridCells {
        AlbumGridCells(
            totalCount: list.totalCount,
            columnCount: columnCount,
            loaded: list.loadedEntries,
            openAlbumId: uiStore.selectedAlbumId
        )
    }

    @ViewBuilder
    private func slot(
        _ cell: AlbumGridCell,
        metrics: AlbumGridMetrics
    ) -> some View {
        switch cell {
        case .album(let position, let albumId):
            albumCard(albumId, width: metrics.cardWidth)
                .task(id: RowLoadID(epoch: list.loadEpoch, index: position)) {
                    await list.loadPage(containing: position)
                }
        case .placeholder(let position):
            AlbumCardPlaceholder(size: metrics.cardWidth)
                .frame(width: metrics.cardWidth)
                .task(id: RowLoadID(epoch: list.loadEpoch, index: position)) {
                    await list.loadPage(containing: position)
                }
        case .detail(let albumId):
            AlbumExpansionSlot(
                albumId: albumId,
                slotWidth: metrics.cardWidth,
                rowWidth: metrics.rowWidth,
                expansionContent: expansionContent
            )
            .transition(.opacity)
        case .filler:
            Color.clear.frame(width: metrics.cardWidth, height: 0)
        }
    }

    /// The album's card, or its placeholder until its summary is interned.
    @ViewBuilder
    private func albumCard(_ albumId: String, width: CGFloat) -> some View {
        if let summary = libraryStore.albumSummaries[albumId] {
            AlbumCardView(
                title: summary.title,
                artistNames: summary.artistNames,
                year: summary.year,
                cover: summary.cover,
                isExpanded: uiStore.selectedAlbumId == albumId,
                isSelected: selection.contains(albumId),
                size: width,
                menu: cardMenu(for: albumId),
            )
            .frame(width: width)
            .draggable(dragPayload(for: albumId))
            .onTapGesture {
                handleTap(on: albumId)
            }
        }
        else {
            AlbumCardPlaceholder(size: width)
                .frame(width: width)
        }
    }
}

extension AlbumGridView {
    /// Scrolls to `albumId` by asking core for its index and loading its page;
    /// the scroll comes last, so a cancelled reveal changes nothing.
    private func revealAlbum(_ albumId: String) async {
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

            // Load the target's page so its card exists to scroll to.
            await list.loadPage(containing: index)
            if Task.isCancelled {
                return
            }

            withAnimation(.easeInOut(duration: 0.3)) {
                scrollPosition.scrollTo(
                    id: AlbumGridCell.Identity.album(albumId),
                    anchor: .top
                )
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
