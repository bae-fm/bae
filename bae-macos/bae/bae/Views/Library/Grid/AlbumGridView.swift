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
    var groupByArtist = false
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
    /// offset, so a change of width keeps that slot in view.
    @State
    private var scrollPosition = ScrollPosition(
        idType: AlbumGridCell.Identity.self
    )
    @State
    private var viewport = AlbumGridViewport()
    /// Which appearance was opened; the album itself is shared by its groups.
    @State
    private var expandedOccurrence: AlbumGridCell.Identity?

    private var expandedSectionId: String? {
        guard let albumId = uiStore.selectedAlbumId else { return nil }
        if case .album(let openedId, let sectionId) = expandedOccurrence,
            openedId == albumId
        {
            return sectionId
        }
        return list.position(of: albumId).flatMap { list.section(at: $0)?.id }
    }

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
                .overlayPreferenceValue(RevealedTrackRowKey.self) { row in
                    GeometryReader { proxy in
                        Color.clear.onChange(
                            of: placedTrackRow(row, in: proxy),
                            initial: true
                        ) { _, placed in
                            viewport.placeRevealedRow(placed)
                            if let placed,
                                viewport.revealedRow(
                                    seq: placed.seq,
                                    atAlbum: true
                                )
                                    != nil
                            {
                                showRevealedRow(placed)
                            }
                        }
                    }
                    .allowsHitTesting(false)
                }
            }
            .scrollPosition($scrollPosition, anchor: .top)
            .onScrollGeometryChange(for: AlbumGridScroll.self) { geometry in
                let top = -geometry.contentInsets.top
                let bottom =
                    geometry.contentSize.height + geometry.contentInsets.bottom
                    - geometry.containerSize.height
                return AlbumGridScroll(
                    offset: geometry.contentOffset.y,
                    visibleHeight: geometry.containerSize.height,
                    offsets: top...max(top, bottom)
                )
            } action: { _, scroll in
                viewport.setScroll(scroll)
                if let seq = viewport.revealArrivedInPlace() {
                    revealArrived(seq: seq)
                }
            }
            // A new column count re-lays every row before the scroll view
            // knows which slot was on top, so the grid names it.
            .onChange(of: metrics.columnCount) { old, new in
                if let slot = viewport.anchor(from: old, to: new) {
                    scrollToTop(slot, columnCount: new)
                }
            }
            // The grid places rows it has not drawn by the heights of the
            // ones it has, and moves them as it draws more, so the held slot
            // goes back on top whenever the content's height changes.
            .onScrollGeometryChange(for: CGFloat.self) { geometry in
                geometry.contentSize.height
            } action: { _, _ in
                keepHeldSlotOnTop(columnCount: metrics.columnCount)
            }
            // Scrolling, another sort, or another open album ends the hold.
            // The person scrolling, or opening another album, also ends a
            // reveal still on its way: what it would scroll to is no longer
            // what they look at.
            .onScrollPhaseChange { old, phase in
                if let seq = viewport.setPhase(from: old, to: phase) {
                    revealArrived(seq: seq)
                }
                if phase != .idle {
                    viewport.release()
                }
                switch phase {
                case .tracking, .interacting, .decelerating:
                    endReveal()
                default:
                    break
                }
            }
            .onChange(of: list.loadEpoch) {
                viewport.release()
            }
            .onChange(of: uiStore.selectedAlbumId) { _, albumId in
                viewport.release()
                if uiStore.pendingAlbumReveal?.albumId != albumId {
                    endReveal()
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
                await show(reveal)
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
    /// Read the row and viewport from the same layout. Scroll callbacks can
    /// arrive after an animation completion and must not position the row.
    ///
    /// The lazy grid places a cell it has just drawn before it grows to hold
    /// it, so a row near the end can sit below the content's bottom for a
    /// layout. The scroll bounds that layout gives stop short of the row, and
    /// a scroll to it would go nowhere; the row counts as placed once the
    /// content holds it.
    private func placedTrackRow(
        _ row: RevealedTrackRow?,
        in proxy: GeometryProxy
    ) -> PlacedTrackRow? {
        guard let row else { return nil }
        guard let bounds = proxy.bounds(of: .scrollView) else {
            preconditionFailure("The album grid must be inside its scroll view")
        }
        let frame = proxy[row.bounds]
        guard frame.maxY <= proxy.size.height + 0.5 else { return nil }
        return PlacedTrackRow(
            trackId: row.trackId,
            seq: row.seq,
            frame: frame,
            scroll: AlbumGridScroll(
                offset: bounds.minY,
                visibleHeight: bounds.height,
                offsets: 0...max(0, proxy.size.height - bounds.height)
            )
        )
    }

    private func cells(columnCount: Int) -> AlbumGridLayout {
        AlbumGridLayout(
            totalCount: list.totalCount,
            sections: groupByArtist ? list.sections : [],
            columnCount: columnCount,
            loaded: list.loadedEntries,
            openAlbumId: uiStore.selectedAlbumId,
            openSectionId: expandedSectionId
        )
    }

    @ViewBuilder
    private func slot(
        _ cell: AlbumGridCell,
        metrics: AlbumGridMetrics
    ) -> some View {
        switch cell {
        case .heading(_, let title):
            Text(verbatim: title)
                .themeText(.rowTitle)
                .padding(.horizontal, ThemeSpace.compact)
                .frame(width: metrics.rowWidth, alignment: .leading)
                .frame(width: metrics.cardWidth, alignment: .leading)
                .accessibilityAddTraits(.isHeader)
        case .album(let position, let albumId, let sectionId):
            placed(
                albumCard(
                    albumId,
                    position: position,
                    sectionId: sectionId,
                    width: metrics.cardWidth
                )
                .task(
                    id: RowLoadID(epoch: list.loadEpoch, index: position)
                ) {
                    await list.holdPage(containing: position)
                },
                cell.id,
                as: .position(position),
                columnCount: metrics.columnCount
            )
        case .placeholder(let position):
            placed(
                AlbumCardPlaceholder(size: metrics.cardWidth)
                    .frame(width: metrics.cardWidth)
                    .task(
                        id: RowLoadID(epoch: list.loadEpoch, index: position)
                    ) {
                        await list.holdPage(containing: position)
                    },
                cell.id,
                as: .position(position),
                columnCount: metrics.columnCount
            )
        case .detail(let albumId):
            placed(
                AlbumExpansionSlot(
                    albumId: albumId,
                    slotWidth: metrics.cardWidth,
                    rowWidth: metrics.rowWidth,
                    expansionContent: expansionContent
                ),
                cell.id,
                as: .detail(albumId: albumId),
                columnCount: metrics.columnCount
            )
            .transition(.opacity)
        case .filler, .sectionFiller:
            Color.clear.frame(width: metrics.cardWidth, height: 0)
        }
    }

    /// Reports where this cell sits in the visible area, and puts the held
    /// slot back on top when the grid moves it.
    private func placed(
        _ view: some View,
        _ cell: AlbumGridCell.Identity,
        as slot: AlbumGridSlot,
        columnCount: Int
    ) -> some View {
        view
            .onGeometryChange(for: CGRect.self) { geometry in
                geometry.frame(in: .scrollView)
            } action: { frame in
                viewport.place(
                    cell,
                    as: slot,
                    columnCount: columnCount,
                    frame: frame
                )
                if viewport.held == slot {
                    keepHeldSlotOnTop(columnCount: columnCount)
                }
                if let seq = viewport.revealArrivedInPlace() {
                    revealArrived(seq: seq)
                }
            }
            .onDisappear {
                viewport.remove(cell, columnCount: columnCount)
            }
    }

    /// The album's card, or its placeholder until its summary is interned.
    @ViewBuilder
    private func albumCard(
        _ albumId: String,
        position: Int,
        sectionId: String?,
        width: CGFloat
    ) -> some View {
        if let summary = libraryStore.albumSummaries[albumId] {
            AlbumCardView(
                title: summary.title,
                artistNames: summary.artistNames,
                year: summary.year,
                cover: summary.cover,
                isExpanded: uiStore.selectedAlbumId == albumId
                    && (!groupByArtist || expandedSectionId == sectionId),
                isSelected: selection.contains(albumId),
                size: width,
                menu: cardMenu(for: albumId),
            )
            .frame(width: width)
            .draggable(dragPayload(for: albumId))
            .onTapGesture {
                handleTap(on: albumId, position: position, sectionId: sectionId)
            }
        }
        else {
            AlbumCardPlaceholder(size: width)
                .frame(width: width)
        }
    }
}

extension AlbumGridView {
    /// Shows what `reveal` names. The album: ask core for its index, hold its
    /// page, and scroll its card to the top; the scroll comes last, so a
    /// cancelled reveal changes nothing. A track: its row, which exists only
    /// once the album's detail is laid out — scrolling to the album is what
    /// brings the detail into the lazy grid — so the reveal stays pending
    /// until `showRevealedRow` scrolls to the row and flashes it.
    ///
    /// The album's page stays held until the reveal ends, which cancels this
    /// task: the rows on screen hold their own pages, and without the hold
    /// the far page the scroll goes to could be dropped before it gets there.
    private func show(_ reveal: PendingAlbumReveal) async {
        // The album's detail may already be laid out with the row in it.
        if let row = viewport.revealedRow(seq: reveal.seq) {
            showRevealedRow(row)
            return
        }
        let getAlbumIndex = library.getAlbumIndex
        let sort = sortCriteria
        let resolved: UInt64?
        do {
            resolved = try await getAlbumIndex(
                sort,
                reveal.albumId,
                groupByArtist
            )
        }
        catch {
            if Task.isCancelled {
                return
            }
            // A failed reveal ends here rather than replaying on every later
            // mount of the grid.
            uiStore.consumeAlbumReveal(seq: reveal.seq)
            uiStore.showError(error)
            return
        }
        if Task.isCancelled {
            return
        }
        guard let index = resolved.map(Int.init) else {
            albumGridLogger.warning(
                "No index for album \(reveal.albumId) under the current sort; skipping reveal"
            )
            uiStore.consumeAlbumReveal(seq: reveal.seq)
            return
        }

        await list.withPage(containing: index) {
            // The reveal may have ended while its page loaded: its row shown,
            // or given up.
            if Task.isCancelled
                || uiStore.pendingAlbumReveal?.seq != reveal.seq
            {
                return
            }
            if let row = viewport.revealedRow(seq: reveal.seq) {
                showRevealedRow(row)
                return
            }
            scrollToAlbum(of: reveal, at: index)
            await Task.untilCancelled()
        }
    }

    /// Scrolls the card of the album `reveal` names, at list position
    /// `index`, to the top. The scroll view's phase says when it comes to
    /// rest: an animated scroll's end is the reveal's arrival at the album.
    private func scrollToAlbum(of reveal: PendingAlbumReveal, at index: Int) {
        // The list moved under the reveal between core placing the album and
        // its page answering: there is no card to scroll to.
        guard list.idAt(index) == reveal.albumId else {
            albumGridLogger.warning(
                "Album \(reveal.albumId) is no longer at \(index); skipping reveal"
            )
            uiStore.consumeAlbumReveal(seq: reveal.seq)
            return
        }
        viewport.release()
        let sectionId = groupByArtist ? list.section(at: index)?.id : nil
        expandedOccurrence = .album(reveal.albumId, sectionId: sectionId)
        let card = AlbumGridCell.Identity.album(
            reveal.albumId,
            sectionId: sectionId
        )
        if let seq = viewport.revealScrolls(seq: reveal.seq, to: card) {
            revealArrived(seq: seq)
            return
        }
        withAnimation(.easeInOut(duration: 0.3)) {
            scrollPosition.scrollTo(id: card, anchor: .top)
        }
    }

    /// The pending reveal `seq` has scrolled to its album, and come to rest
    /// there. An album shown alone ends its reveal here. A track's row is
    /// measured from here: where the scroll stops, not on its way.
    private func revealArrived(seq: Int) {
        guard let reveal = uiStore.pendingAlbumReveal, reveal.seq == seq else {
            return
        }
        guard reveal.trackId != nil else {
            uiStore.consumeAlbumReveal(seq: seq)
            return
        }
        if let row = viewport.revealedRow(seq: seq, atAlbum: true) {
            showRevealedRow(row)
        }
    }

    /// Scrolls the row the pending reveal names into full view, if it is not
    /// there already, then flashes it. The reveal ends when its scroll starts.
    private func showRevealedRow(_ row: PlacedTrackRow) {
        guard uiStore.pendingAlbumReveal?.seq == row.seq else {
            return
        }
        uiStore.consumeAlbumReveal(seq: row.seq)
        guard let offset = row.scrollOffset else {
            uiStore.flashTrack(row.trackId, seq: row.seq)
            return
        }
        viewport.release()
        withAnimation(.easeInOut(duration: 0.3)) {
            scrollPosition.scrollTo(y: offset)
        } completion: {
            uiStore.flashTrack(row.trackId, seq: row.seq)
        }
    }

    /// Gives up the pending reveal: the person moved on before it was shown.
    private func endReveal() {
        if let reveal = uiStore.pendingAlbumReveal {
            uiStore.consumeAlbumReveal(seq: reveal.seq)
        }
    }

    /// Scrolls the held slot back to the top when the grid moved it.
    private func keepHeldSlotOnTop(columnCount: Int) {
        guard let slot = viewport.held,
            !viewport.isOnTop(slot, columnCount: columnCount)
        else { return }
        scrollToTop(slot, columnCount: columnCount)
    }

    /// Scrolls `slot` to the top, at its place under `columnCount` columns.
    private func scrollToTop(_ slot: AlbumGridSlot, columnCount: Int) {
        let id: AlbumGridCell.Identity
        switch slot {
        case .position(let position) where position < list.totalCount:
            if let albumId = list.idAt(position) {
                id = .album(
                    albumId,
                    sectionId: groupByArtist
                        ? list.section(at: position)?.id : nil
                )
            }
            else {
                id = .placeholder(position)
            }
        case .detail(let albumId) where uiStore.selectedAlbumId == albumId:
            id = .detail
        case .position, .detail:
            // The slot left the grid; the next change keeps what is on top
            // then.
            viewport.release()
            return
        }
        scrollPosition.scrollTo(id: id, anchor: .top)
    }

    /// Cmd toggles the album in the selection, shift extends the range, and a
    /// plain click clears the selection and toggles the album's detail.
    private func handleTap(
        on albumId: String,
        position: Int,
        sectionId: String?
    ) {
        let modifiers = NSEvent.modifierFlags
        if modifiers.contains(.command) {
            selection.setSelected(
                albumId,
                selected: !selection.contains(albumId),
                sectionId: sectionId
            )
            gridFocused = true
        }
        else if modifiers.contains(.shift) {
            selection.extendRange(
                to: albumId,
                targetPosition: position,
                targetSectionId: sectionId,
                position: {
                    list.position(of: $0, in: selection.anchor?.sectionId)
                },
                idAt: { list.idAt($0) }
            )
            gridFocused = true
        }
        else {
            selection.clear()
            withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) {
                let isOpen =
                    uiStore.selectedAlbumId == albumId
                    && (!groupByArtist || expandedSectionId == sectionId)
                expandedOccurrence = .album(albumId, sectionId: sectionId)
                uiStore.selectAlbum(
                    isOpen ? nil : albumId
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
