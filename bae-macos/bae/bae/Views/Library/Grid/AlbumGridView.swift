import BaeKit
import SwiftUI
import os.log

private let albumGridLogger = Logger.bae("AlbumGridView")
private let albumCardSize: CGFloat = 200
private let gridSpacing = ThemeSpace.page

struct AlbumGridView<ExpansionContent: View>: View {
    @Environment(UiStore.self)
    private var uiStore
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
    /// Bulk actions, each given the target albums in grid order, as the
    /// grid's query returned them.
    /// Each acts on album ids in no order; the receiver has core order them
    /// as the grid lists them.
    let onPlay: ([String]) -> Void
    let onAddToQueue: ([String]) -> Void
    let onAddNext: ([String]) -> Void
    /// Ask to delete one album.
    let onDeleteAlbum: (String) -> Void
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
    /// The card the person opened or a reveal scrolled to. An album grouped
    /// by artist has a card in each of its artists' groups, and its detail
    /// opens under the one opened; an album selected elsewhere opens under
    /// its first card.
    @State
    private var openedCard: OpenedCard?
    /// Where core places the open album's card, read live.
    @State
    private var openPlace = AlbumGridOpenPlace()

    /// The list the grid shows now, which the selection's anchor position is
    /// in.
    private var listing: AlbumGridListing {
        AlbumGridListing(epoch: list.loadEpoch, rows: list.positionCount)
    }

    /// What the grid asks core to place: the open album, in the group the
    /// person opened it under.
    private var placeRequest: AlbumPlaceRequest? {
        guard let albumId = uiStore.selectedAlbumId else { return nil }
        let sectionId =
            groupByArtist && openedCard?.albumId == albumId
            ? openedCard?.sectionId : nil
        return AlbumPlaceRequest(
            albumId: albumId,
            sectionId: sectionId,
            sortCriteria: sortCriteria,
            groupByArtist: groupByArtist
        )
    }

    /// Where core places the open album's card, while that album is open.
    private var openAnswer:
        (request: AlbumPlaceRequest, place: BridgeAlbumPlace)?
    {
        guard let answer = openPlace.answer,
            answer.request.albumId == uiStore.selectedAlbumId
        else { return nil }
        return answer
    }

    /// The open album's card at the position core places it.
    private var openCard: AlbumGridOpenCard? {
        openAnswer.map {
            AlbumGridOpenCard(
                albumId: $0.request.albumId,
                position: Int($0.place.position)
            )
        }
    }

    private var expandedSectionId: String? {
        openAnswer?.place.sectionId
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
                // The row's place as each layout leaves it. An onChange of
                // it acts at most once a frame and loses a later place laid
                // out in the same frame, which the reveal measures the row at.
                .overlayPreferenceValue(RevealedTrackRowKey.self) { row in
                    Color.clear
                        .onGeometryChange(
                            for: PlacedTrackRow?.self
                        ) { proxy in
                            Self.placedTrackRow(row, in: proxy)
                        } action: { placed in
                            viewport.placeRevealedRow(placed)
                            albumGridLogger.debug(
                                "placed row changed: \(String(describing: placed))"
                            )
                            if let placed,
                                viewport.revealedRow(
                                    seq: placed.seq,
                                    arrived: true
                                )
                                    != nil
                            {
                                showRevealedRow(placed)
                            }
                        }
                        .allowsHitTesting(false)
                }
            }
            .accessibilityIdentifier("album-grid")
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
                if let reveal = uiStore.pendingAlbumReveal {
                    albumGridLogger.debug(
                        "reveal \(reveal.seq): scroll \(String(describing: scroll))"
                    )
                }
                if let seq = viewport.revealArrivedInPlace() {
                    revealArrived(seq: seq)
                }
            }
            // The list sizes its window from the slots the grid fits before
            // any card arrives: every column of each row that fits.
            .onChange(
                of: GridScreen(
                    height: geometry.size.height,
                    rowHeight: metrics.rowHeight,
                    columns: metrics.columnCount
                ),
                initial: true
            ) { _, screen in
                list.fit(
                    height: screen.height,
                    rowHeight: screen.rowHeight,
                    columns: screen.columns
                )
            }
            // A new column count re-lays every row before the scroll view
            // knows which slot was on top, so the grid names it.
            .onChange(of: metrics.columnCount) { old, new in
                if let slot = viewport.anchor(from: old, to: new) {
                    scrollToTop(slot)
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
            .onScrollPhaseChange { _, phase in
                if let reveal = uiStore.pendingAlbumReveal {
                    albumGridLogger.debug(
                        "reveal \(reveal.seq): scroll phase \(String(describing: phase))"
                    )
                }
                if let seq = viewport.setPhase(phase) {
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
            // The selection's anchor is a position in the list the grid
            // showed when it was clicked.
            .onChange(of: listing) {
                selection.dropAnchor()
            }
            .task(id: placeRequest) {
                guard let request = placeRequest else {
                    openPlace.clear()
                    return
                }
                await openPlace.follow(request, in: library) { error in
                    uiStore.showError(error)
                }
            }
            .onChange(of: uiStore.selectedAlbumId) { _, albumId in
                viewport.release()
                if uiStore.pendingAlbumReveal?.albumId != albumId {
                    albumGridLogger.debug(
                        "selected album changed to \(String(describing: albumId)); ending the reveal"
                    )
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

/// The grid's height, its row height and its columns, which size the list's
/// window.
private struct GridScreen: Equatable {
    let height: CGFloat
    let rowHeight: CGFloat
    let columns: Int
}

/// One load of the grid's list at one row count. The selection's anchor
/// position names a card in the listing it was taken in: another sort or
/// grouping loads the list anew, and a write that adds or removes rows moves
/// the rows after it.
private struct AlbumGridListing: Hashable {
    let epoch: LoadEpoch
    let rows: Int
}

/// The card the grid opened: its album, and the artist's group it is in when
/// the grid groups by artist.
private struct OpenedCard: Equatable {
    let albumId: String
    let sectionId: String?
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

    /// The height the list counts a row of cards as when it sizes its
    /// window: a card's cover and the gap under it, which is shorter than
    /// the card.
    var rowHeight: CGFloat {
        cardWidth + gridSpacing
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
    /// arrive from another layout and must not position the row.
    ///
    /// The lazy grid places a cell it has just drawn before it grows to hold
    /// it, so a row near the end can sit below the content's bottom for a
    /// layout. The scroll bounds that layout gives stop short of the row, and
    /// a scroll to it would go nowhere; the row counts as placed once the
    /// content holds it.
    private nonisolated static func placedTrackRow(
        _ row: RevealedTrackRow?,
        in proxy: GeometryProxy
    ) -> PlacedTrackRow? {
        guard let row else { return nil }
        guard let bounds = proxy.bounds(of: .scrollView) else {
            preconditionFailure("The album grid must be inside its scroll view")
        }
        let frame = proxy[row.bounds]
        guard frame.maxY <= proxy.size.height + 0.5 else {
            albumGridLogger.debug(
                "reveal \(row.seq): row \(row.trackId) laid out below the content: row \(String(describing: frame)), content height \(proxy.size.height), scroll view \(String(describing: bounds)); not placed"
            )
            return nil
        }
        albumGridLogger.debug(
            "reveal \(row.seq): row \(row.trackId) placed: row \(String(describing: frame)), content height \(proxy.size.height), scroll view \(String(describing: bounds))"
        )
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

    /// The grid's slots, each album read from the rows the list's view
    /// delivered last. The grid reads the delivery here, as it draws, so it
    /// draws its slots again when the next delivery arrives.
    private func cells(columnCount: Int) -> AlbumGridLayout {
        let delivered = list.delivered
        return AlbumGridLayout(
            totalCount: list.positionCount,
            sections: groupByArtist ? list.sections : [],
            columnCount: columnCount,
            idAt: { delivered?.row(at: $0)?.id },
            open: openCard
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
                as: .position(position, sectionId: sectionId),
                columnCount: metrics.columnCount
            )
        case .placeholder(let position, let sectionId):
            placed(
                AlbumCardPlaceholder(size: metrics.cardWidth)
                    .frame(width: metrics.cardWidth)
                    .task(
                        id: RowLoadID(epoch: list.loadEpoch, index: position)
                    ) {
                        await list.holdPage(containing: position)
                    },
                cell.id,
                as: .position(position, sectionId: sectionId),
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

    /// The album's card, drawn from the row the grid's query returned at
    /// `position`, or its placeholder until that row's page lands.
    @ViewBuilder
    private func albumCard(
        _ albumId: String,
        position: Int,
        sectionId: String?,
        width: CGFloat
    ) -> some View {
        if let album = list.row(at: position) {
            MacAlbumTile(
                title: album.title,
                artistNames: album.artistLine.text,
                year: album.year,
                cover: album.cover,
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
        albumGridLogger.debug(
            "reveal \(reveal.seq): album \(reveal.albumId), track \(String(describing: reveal.trackId)); started"
        )
        // The album's detail may already be laid out with the row in it.
        if let row = viewport.revealedRow(seq: reveal.seq) {
            albumGridLogger.debug(
                "reveal \(reveal.seq): the row is already laid out"
            )
            showRevealedRow(row)
            return
        }
        let albumPlace = library.albumPlace
        let sort = sortCriteria
        let resolved: BridgeAlbumPlace?
        do {
            resolved = try await albumPlace(
                sort,
                reveal.albumId,
                groupByArtist,
                nil
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
        guard let place = resolved else {
            albumGridLogger.warning(
                "No index for album \(reveal.albumId) under the current sort; skipping reveal"
            )
            uiStore.consumeAlbumReveal(seq: reveal.seq)
            return
        }
        let index = Int(place.position)

        await list.withPage(containing: index) {
            // The reveal may have ended while its page loaded: its row shown,
            // or given up.
            if Task.isCancelled
                || uiStore.pendingAlbumReveal?.seq != reveal.seq
            {
                return
            }
            if let row = viewport.revealedRow(seq: reveal.seq) {
                albumGridLogger.debug(
                    "reveal \(reveal.seq): the row is laid out once the page is held"
                )
                showRevealedRow(row)
                return
            }
            albumGridLogger.debug(
                "reveal \(reveal.seq): scrolling to the album at \(index)"
            )
            scrollToAlbum(of: reveal, at: index, in: place.sectionId)
            await Task.untilCancelled()
        }
    }

    /// Scrolls the card of the album `reveal` names, at list position
    /// `index` in the group of the artist `sectionId`, to the top. The reveal arrives at the album once the card is
    /// laid out on top.
    ///
    /// The scroll does not animate. An animated scroll to the card left the
    /// scroll view's position on the card after the scroll stopped, so the
    /// scroll to the track's row that followed was undone: the grid went back
    /// to the card with the row below the visible area.
    private func scrollToAlbum(
        of reveal: PendingAlbumReveal,
        at index: Int,
        in sectionId: String?
    ) {
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
        openedCard = OpenedCard(albumId: reveal.albumId, sectionId: sectionId)
        let card = AlbumGridCell.Identity.album(
            reveal.albumId,
            sectionId: sectionId
        )
        if let seq = viewport.revealScrolls(seq: reveal.seq, to: card) {
            revealArrived(seq: seq)
            return
        }
        // The scroll lands where the lazy grid estimated the card, and the
        // grid moves the card as it draws the rows around it; the hold puts
        // the card back on top each time.
        viewport.hold(.position(index, sectionId: sectionId))
        scrollPosition.scrollTo(id: card, anchor: .top)
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
        if let row = viewport.revealedRow(seq: seq, arrived: true) {
            albumGridLogger.debug(
                "reveal \(seq): arrived at the album; the row is laid out"
            )
            showRevealedRow(row)
        }
        else {
            albumGridLogger.debug(
                "reveal \(seq): arrived at the album; waiting for the row"
            )
        }
    }

    /// Flashes the row the pending reveal names where it is in full view, as
    /// far as the scroll can bring it, and ends the reveal; otherwise
    /// scrolls it there. The reveal stays pending through that scroll and
    /// measures the row again where the grid lays it out next: a far scroll
    /// lands over several layouts of the lazy grid, and the header above the
    /// grid folds away as the grid leaves its top, resizing the grid under
    /// the scroll.
    private func showRevealedRow(_ row: PlacedTrackRow) {
        guard uiStore.pendingAlbumReveal?.seq == row.seq else {
            return
        }
        guard let offset = row.scrollOffset else {
            albumGridLogger.debug(
                "reveal \(row.seq): row \(row.trackId) needs no scroll (\(String(describing: row))); flashing it"
            )
            uiStore.consumeAlbumReveal(seq: row.seq)
            uiStore.flashTrack(row.trackId, seq: row.seq)
            return
        }
        albumGridLogger.debug(
            "reveal \(row.seq): scrolling to y \(offset) for row \(row.trackId)"
        )
        viewport.revealScrollsToRow(seq: row.seq)
        viewport.release()
        scrollPosition.scrollTo(y: offset)
    }

    /// Gives up the pending reveal: the person moved on before it was shown.
    private func endReveal() {
        if let reveal = uiStore.pendingAlbumReveal {
            albumGridLogger.debug("reveal \(reveal.seq): given up")
            uiStore.consumeAlbumReveal(seq: reveal.seq)
        }
    }

    /// Scrolls the held slot back to the top when the grid moved it.
    private func keepHeldSlotOnTop(columnCount: Int) {
        guard
            let slot = viewport.scrollBackingHeldSlot(columnCount: columnCount)
        else { return }
        scrollToTop(slot)
    }

    /// Scrolls `slot` to the top.
    private func scrollToTop(_ slot: AlbumGridSlot) {
        let id: AlbumGridCell.Identity
        switch slot {
        case .position(let position, let sectionId)
        where position < list.positionCount:
            if let albumId = list.idAt(position) {
                id = .album(albumId, sectionId: sectionId)
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
                position: position,
                sectionId: sectionId
            )
            gridFocused = true
        }
        else if modifiers.contains(.shift) {
            selection.extendRange(
                to: albumId,
                position: position,
                sectionId: sectionId,
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
                openedCard = OpenedCard(albumId: albumId, sectionId: sectionId)
                uiStore.selectAlbum(
                    isOpen ? nil : albumId
                )
            }
        }
    }

    /// The card's menu, acting on the whole selection when the card is in it.
    private func cardMenu(for albumId: String) -> AlbumCardMenu {
        let targets = selection.targets(for: albumId)
        return AlbumCardMenu(
            targetCount: targets.count,
            onPlay: { onPlay(targets) },
            onAddToQueue: { onAddToQueue(targets) },
            onAddNext: { onAddNext(targets) },
            onDelete: targets.count == 1 ? { onDeleteAlbum(albumId) } : nil
        )
    }

    /// The card's drag payload: the whole selection when the card is in it.
    private func dragPayload(for albumId: String) -> String {
        AlbumDragPayload.encode(selection.targets(for: albumId))
    }
}

#if DEBUG
    // MARK: - Previews

    private struct GridPreview: View {
        let width: CGFloat
        let height: CGFloat
        private let sortCriteria: [BridgeSortCriterion] = [
            BridgeSortCriterion(field: .dateAdded, direction: .descending)
        ]

        var body: some View {
            let list = AlbumList.preview(albums: PreviewData.albums)
            AlbumGridView(
                list: list,
                sortCriteria: sortCriteria,
                fullWidth: false,
                selection: AlbumGridSelection(),
                onPlay: { _ in },
                onAddToQueue: { _ in },
                onAddNext: { _ in },
                onDeleteAlbum: { _ in },
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
        GridPreview(width: 1100, height: 700)
            .albumDetailPreviewEnvironment()
    }

    #Preview("Grid \u{2014} Medium") {
        GridPreview(width: 700, height: 600)
            .albumDetailPreviewEnvironment()
    }

    #Preview("Grid \u{2014} Narrow") {
        GridPreview(width: 400, height: 600)
            .albumDetailPreviewEnvironment()
    }
#endif
