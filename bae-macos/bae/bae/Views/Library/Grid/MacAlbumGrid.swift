import BaeKit
import Combine
import SwiftUI
import os.log

private let albumGridLogger = Logger.bae("MacAlbumGrid")
private let albumCardSize: CGFloat = 200
private let gridSpacing = ThemeSpace.page

struct MacAlbumGrid<ExpansionContent: View>: View {
    @Environment(UiStore.self)
    private var uiStore
    @Environment(Library.self)
    private var library
    let list: AlbumList
    /// The active sort, under which core places the open album's card.
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
    /// The reveal the grid took from `UiStore.albumReveals` and has not
    /// shown yet. It follows where core places the album's card, and ends
    /// once the grid has shown the card or flashed the row, or sooner when
    /// the grid holds no card of the album, cannot read where the card is,
    /// or the person scrolls, clicks a card or opens another album.
    @State
    private var reveal: AlbumReveal?

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
            answer.request.albumId == uiStore.selectedAlbumId,
            let place = answer.place
        else { return nil }
        return (answer.request, place)
    }

    /// The reveal and what core answered for where its album's card is: a
    /// reveal names the open album, so the open album's place is the card the
    /// grid scrolls to. The reveal starts again each time core places the
    /// card elsewhere before the grid has shown it.
    private var revealTarget: RevealTarget? {
        guard let reveal else { return nil }
        guard let answer = openPlace.answer,
            answer.request.albumId == reveal.albumId
        else { return RevealTarget(reveal: reveal, place: .waiting) }
        return RevealTarget(
            reveal: reveal,
            place: answer.place.map(RevealPlace.card) ?? .noCard
        )
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
                .coordinateSpace(.named(albumGridContentSpace))
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
                                    of: placed.trackId,
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
            } action: { old, scroll in
                viewport.setScroll(scroll)
                // The grid places rows it has not drawn by the heights of
                // the ones it has, and moves them as it draws more, and the
                // content's end can stop a slot short of the top until the
                // rows under it are drawn; so the held slot goes back on top
                // whenever the offsets the content scrolls between change.
                // The viewport takes the new offsets first, so the hold
                // measures against them.
                if old.offsets != scroll.offsets {
                    keepHeldSlotOnTop(columnCount: metrics.columnCount)
                }
                if let reveal {
                    albumGridLogger.debug(
                        "reveal of \(reveal.albumId): scroll \(String(describing: scroll))"
                    )
                }
                if viewport.revealArrivedInPlace() {
                    revealArrived()
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
            // Scrolling, another sort, or another open album ends the hold.
            // The person scrolling also ends the reveal: the grid no longer
            // shows what they asked to see.
            .onScrollPhaseChange { _, phase in
                if let reveal {
                    albumGridLogger.debug(
                        "reveal of \(reveal.albumId): scroll phase \(String(describing: phase))"
                    )
                }
                if viewport.setPhase(phase) {
                    revealArrived()
                }
                if phase != .idle {
                    viewport.release()
                }
                switch phase {
                case .tracking, .interacting, .decelerating:
                    reveal = nil
                default:
                    break
                }
            }
            .onChange(of: list.loadEpoch) {
                viewport.release()
            }
            .task(id: placeRequest) {
                guard let request = placeRequest else {
                    openPlace.clear()
                    return
                }
                await openPlace.follow(request, in: library) { error in
                    // The grid cannot show the album without its place.
                    endReveal(of: request.albumId)
                    uiStore.showError(error)
                }
            }
            .onChange(of: uiStore.selectedAlbumId) { _, albumId in
                viewport.release()
                // Another album open, or none: the person moved on.
                if let reveal, reveal.albumId != albumId {
                    self.reveal = nil
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
            .onReceive(uiStore.albumReveals.requests) { request in
                reveal = request
            }
            .task(id: revealTarget) {
                guard let target = revealTarget else { return }
                await show(target.reveal, at: target.place)
            }
        }
    }
}

/// The grid content's coordinate space, in which a scroll to an offset
/// measures.
private let albumGridContentSpace = "albumGridContent"

/// The grid's height, its row height and its columns, which size the list's
/// window.
private struct GridScreen: Equatable {
    let height: CGFloat
    let rowHeight: CGFloat
    let columns: Int
}

/// A reveal, with what core answered for where its album's card is.
private struct RevealTarget: Equatable {
    let reveal: AlbumReveal
    let place: RevealPlace
}

/// What core answered for where the card of a reveal's album is.
private enum RevealPlace: Equatable {
    /// Core has not answered for the album yet.
    case waiting
    /// The grid shows no card of the album.
    case noCard
    case card(BridgeAlbumPlace)
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

extension MacAlbumGrid {
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
                "reveal: row \(row.trackId) laid out below the content: row \(String(describing: frame)), content height \(proxy.size.height), scroll view \(String(describing: bounds)); not placed"
            )
            return nil
        }
        albumGridLogger.debug(
            "reveal: row \(row.trackId) placed: row \(String(describing: frame)), content height \(proxy.size.height), scroll view \(String(describing: bounds))"
        )
        return PlacedTrackRow(
            trackId: row.trackId,
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
        case .heading(let section):
            MacArtistGroupHeading(
                name: section.title,
                picture: section.image,
                countLabel: section.countLine
            )
            .frame(width: metrics.rowWidth, alignment: .leading)
            .frame(width: metrics.cardWidth, alignment: .leading)
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
                AlbumCardPlaceholder(width: metrics.cardWidth)
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
                )
                .environment(\.revealedTrackId, reveal?.trackId),
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
            .onGeometryChange(for: AlbumGridPlacedCell.self) { geometry in
                AlbumGridPlacedCell(
                    frame: geometry.frame(in: .scrollView),
                    contentTop:
                        geometry.frame(
                            in: .named(albumGridContentSpace)
                        )
                        .minY
                )
            } action: { placed in
                viewport.place(
                    cell,
                    as: slot,
                    columnCount: columnCount,
                    placed: placed
                )
                if viewport.held == slot {
                    keepHeldSlotOnTop(columnCount: columnCount)
                }
                if viewport.revealArrivedInPlace() {
                    revealArrived()
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
        if let appearance = list.row(at: position) {
            let album = appearance.album
            MacAlbumTileContainer(
                album: album,
                artistNames: appearance.displayArtistLine?.text,
                isExpanded: uiStore.selectedAlbumId == albumId
                    && (!groupByArtist || expandedSectionId == sectionId),
                isSelected: selection.contains(albumId),
                isUnplayable: appearance.primaryReleaseUnplayable,
                width: width,
                menu: cardMenu(for: albumId),
            )
            .draggable(dragPayload(for: albumId))
            .onTapGesture {
                handleTap(on: albumId, sectionId: sectionId)
            }
        }
        else {
            AlbumCardPlaceholder(width: width)
        }
    }
}

extension MacAlbumGrid {
    /// Shows what `reveal` names. The album: hold the page of the card at
    /// `place`, where core places it, and scroll the card to the top; the
    /// scroll comes last, so a cancelled task changes nothing. A track: its
    /// row, which exists only once the album's detail is laid out — scrolling
    /// to the album is what brings the detail into the lazy grid — so the
    /// grid scrolls to the row and flashes it once the row is laid out
    /// (`showRevealedRow`). The reveal ends once the grid has shown what it
    /// names.
    ///
    /// The album's page stays held until the reveal ends or core places the
    /// card elsewhere, either of which cancels this task: the rows on screen
    /// hold their own pages, and without the hold the far page the scroll
    /// goes to could be dropped before it gets there.
    private func show(_ reveal: AlbumReveal, at place: RevealPlace) async {
        viewport.endRevealScroll()
        let card: BridgeAlbumPlace
        switch place {
        case .waiting:
            // The grid shows the reveal once core answers.
            return
        case .noCard:
            // Nothing to show, so nothing for the reveal to wait for.
            albumGridLogger.warning(
                "No card of album \(reveal.albumId) under the current sort; ending its reveal"
            )
            endReveal(of: reveal.albumId)
            return
        case .card(let placed):
            card = placed
        }
        albumGridLogger.debug(
            "reveal of \(reveal.albumId), track \(String(describing: reveal.trackId)): showing it at \(String(describing: card))"
        )
        // The album's detail may already be laid out with the row in it.
        if let row = revealedRow(of: reveal) {
            albumGridLogger.debug(
                "reveal of \(reveal.albumId): the row is already laid out"
            )
            showRevealedRow(row)
            return
        }
        let index = Int(card.position)

        await list.withPage(containing: index) {
            if Task.isCancelled {
                return
            }
            if let row = revealedRow(of: reveal) {
                albumGridLogger.debug(
                    "reveal of \(reveal.albumId): the row is laid out once the page is held"
                )
                showRevealedRow(row)
                return
            }
            albumGridLogger.debug(
                "reveal of \(reveal.albumId): scrolling to the album at \(index)"
            )
            scrollToAlbum(reveal.albumId, at: index, in: card.sectionId)
            await Task.untilCancelled()
        }
    }

    /// The row of the track `reveal` names, laid out.
    private func revealedRow(of reveal: AlbumReveal) -> PlacedTrackRow? {
        reveal.trackId.flatMap { viewport.revealedRow(of: $0) }
    }

    /// Scrolls the card of the revealed album `albumId`, at list position
    /// `index` in the group of the artist `sectionId`, to the top. The grid
    /// arrives at the album once the card is laid out on top.
    ///
    /// The scroll does not animate. An animated scroll to the card left the
    /// scroll view's position on the card after the scroll stopped, so the
    /// scroll to the track's row that followed was undone: the grid went back
    /// to the card with the row below the visible area.
    private func scrollToAlbum(
        _ albumId: String,
        at index: Int,
        in sectionId: String?
    ) {
        viewport.release()
        let card = AlbumGridCell.Identity.album(albumId, sectionId: sectionId)
        if viewport.revealScrolls(to: card) {
            revealArrived()
            return
        }
        // The scroll lands where the lazy grid estimated the card, and the
        // grid moves the card as it draws the rows around it; the hold puts
        // the card back on top each time.
        viewport.holdForReveal(.position(index, sectionId: sectionId))
        scrollPosition.scrollTo(id: card, anchor: .top)
    }

    /// The grid has scrolled to the revealed album, and come to rest there.
    /// An album revealed alone is shown, which ends its reveal. A track's row
    /// is measured from here: where the scroll stops, not on its way.
    private func revealArrived() {
        guard let reveal else { return }
        guard let trackId = reveal.trackId else {
            albumGridLogger.debug("reveal of \(reveal.albumId): shown")
            viewport.endRevealScroll()
            endReveal(of: reveal.albumId)
            return
        }
        if let row = viewport.revealedRow(of: trackId, arrived: true) {
            albumGridLogger.debug(
                "reveal of \(reveal.albumId): arrived at the album; the row is laid out"
            )
            showRevealedRow(row)
        }
        else {
            albumGridLogger.debug(
                "reveal of \(reveal.albumId): arrived at the album; waiting for the row"
            )
        }
    }

    /// Flashes the revealed track's row where it is in full view, as far as
    /// the scroll can bring it, which ends the reveal; otherwise scrolls it
    /// there. The grid measures the row again where it lays it out next: a
    /// far scroll lands over several layouts of the lazy grid, and the header
    /// above the grid folds away as the grid leaves its top, resizing the
    /// grid under the scroll.
    private func showRevealedRow(_ row: PlacedTrackRow) {
        guard let reveal, reveal.trackId == row.trackId else { return }
        guard let offset = row.scrollOffset else {
            albumGridLogger.debug(
                "reveal: row \(row.trackId) needs no scroll (\(String(describing: row))); flashing it"
            )
            viewport.endRevealScroll()
            endReveal(of: reveal.albumId)
            uiStore.flashTrack(row.trackId)
            return
        }
        albumGridLogger.debug(
            "reveal: scrolling to y \(offset) for row \(row.trackId)"
        )
        viewport.revealScrollsToRow()
        viewport.release()
        scrollPosition.scrollTo(y: offset)
    }

    /// Ends the reveal of `albumId`; a reveal of another album that came
    /// meanwhile stays.
    private func endReveal(of albumId: String) {
        if reveal?.albumId == albumId {
            reveal = nil
        }
    }

    /// Scrolls the held slot back to the top when the grid moved it, as
    /// what holds it says (`AlbumGridHold`): a revealed card to its top in
    /// the content, or by its cell while it is not laid out and so has no
    /// top in the content.
    private func keepHeldSlotOnTop(columnCount: Int) {
        guard
            let held = viewport.scrollBackingHeldSlot(columnCount: columnCount)
        else { return }
        if held.hold.reason == .reveal, let contentTop = held.contentTop {
            scrollPosition.scrollTo(y: contentTop)
        }
        else {
            scrollToTop(held.hold.slot)
        }
    }

    /// Scrolls `slot` to the top by its cell.
    private func scrollToTop(_ slot: AlbumGridSlot) {
        guard let id = cell(at: slot) else {
            // The slot left the grid; the next change keeps what is on top
            // then.
            viewport.release()
            return
        }
        scrollPosition.scrollTo(id: id, anchor: .top)
    }

    /// The cell that shows `slot`, or nil when the slot has left the grid.
    private func cell(at slot: AlbumGridSlot) -> AlbumGridCell.Identity? {
        switch slot {
        case .position(let position, let sectionId)
        where position < list.positionCount:
            if let albumId = list.idAt(position) {
                return .album(albumId, sectionId: sectionId)
            }
            return .placeholder(position)
        case .detail(let albumId) where uiStore.selectedAlbumId == albumId:
            return .detail
        case .position, .detail:
            return nil
        }
    }

    /// Cmd toggles the album in the selection, shift extends the range, and a
    /// plain click clears the selection and toggles the album's detail.
    private func handleTap(on albumId: String, sectionId: String?) {
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
            let between = library.albumIdsBetween
            let sort = sortCriteria
            let grouped = groupByArtist
            selection.extendRange(
                to: albumId,
                sectionId: sectionId,
                readRange: { from, to in
                    try await between(
                        sort,
                        grouped,
                        from.albumId,
                        from.sectionId,
                        to.albumId,
                        to.sectionId
                    )
                },
                onFailure: { uiStore.showError($0) }
            )
            gridFocused = true
        }
        else {
            selection.clear()
            // The person chose what to look at.
            reveal = nil
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
            MacAlbumGrid(
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
