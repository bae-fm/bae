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
    @State
    private var scrollOffset = AlbumGridScrollOffset()
    /// Which appearance was opened; the album itself is shared by its groups.
    @State
    private var expandedOccurrence: AlbumOccurrence?
    /// The list position of the album the pending reveal shows, once core has
    /// placed it: that card reports where it is laid out.
    @State
    private var revealCard: RevealCard?

    private var expandedSectionId: String? {
        guard let albumId = uiStore.selectedAlbumId else { return nil }
        if let expandedOccurrence, expandedOccurrence.albumId == albumId {
            return expandedOccurrence.sectionId
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
                // One lazy grid over every slot, keyed by list position: a
                // new column count moves each card to its new place instead
                // of making new cards in other rows.
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
                .transformAnchorPreference(
                    key: RevealAnchorsKey.self,
                    value: .bounds
                ) { anchors, bounds in
                    anchors.content = bounds
                }
            }
            .overlayPreferenceValue(RevealAnchorsKey.self) { anchors in
                RevealMeter(
                    anchors: anchors,
                    scrollOffset: scrollOffset,
                    measure: revealLayout
                ) { layout in
                    viewport.placeReveal(layout)
                    if let reveal = uiStore.pendingAlbumReveal {
                        stepReveal(reveal)
                    }
                }
                .allowsHitTesting(false)
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
                scrollOffset.y = scroll.offset
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

/// Where the grid is scrolled, as the scroll view reports it. Only the
/// reveal's measurement reads it: a scroll alone moves the content and lays
/// out nothing in it, so nothing else would measure the reveal's parts
/// again.
@MainActor
@Observable
private final class AlbumGridScrollOffset {
    var y: CGFloat = 0
}

/// Measures the pending reveal's parts over the scroll view, again whenever
/// the content is laid out and whenever it scrolls.
private struct RevealMeter: View {
    let anchors: RevealAnchors
    let scrollOffset: AlbumGridScrollOffset
    let measure: (RevealAnchors, GeometryProxy) -> RevealLayout?
    let measured: (RevealLayout?) -> Void

    var body: some View {
        // Read for its changes alone: a scroll moves the content, which
        // changes no anchor, so nothing else would measure again.
        measurement(scrolledTo: scrollOffset.y)
    }

    private func measurement(scrolledTo _: CGFloat) -> some View {
        GeometryReader { proxy in
            Color.clear.onChange(
                of: measure(anchors, proxy),
                initial: true
            ) { _, layout in
                measured(layout)
            }
        }
    }
}

/// One appearance of an album in the grid: in the artist section it was
/// opened under, when the grid groups albums by artist.
private struct AlbumOccurrence: Equatable {
    let albumId: String
    let sectionId: String?
}

/// The list position of the album reveal `seq` shows.
private struct RevealCard: Equatable {
    let seq: Int
    let position: Int
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
    /// The pending reveal's parts and where the scroll stands, read from one
    /// layout of the scroll view: each in the content's coordinates, from
    /// where the content sits in the scroll view. Callbacks of their own
    /// report each at its own time, and a scroll's phase arrives after the
    /// layouts it moved; read together here they always agree.
    ///
    /// The lazy grid places a cell it has just drawn before it grows to hold
    /// it, so a part near the end can sit below the content's bottom for a
    /// layout. The scroll bounds that layout gives stop short of it, and a
    /// scroll to it would go nowhere; a part counts once the content holds
    /// it.
    private func revealLayout(
        _ anchors: RevealAnchors,
        in proxy: GeometryProxy
    ) -> RevealLayout? {
        guard let seq = uiStore.pendingAlbumReveal?.seq,
            let contentAnchor = anchors.content
        else { return nil }
        let content = proxy[contentAnchor]
        let visibleHeight = proxy.size.height
        func frame(_ part: RevealAnchors.Part?) -> CGRect? {
            guard let part, part.seq == seq else { return nil }
            let frame = proxy[part.bounds].offsetBy(dx: 0, dy: -content.minY)
            return frame.maxY <= content.height + 0.5 ? frame : nil
        }
        return RevealLayout(
            seq: seq,
            card: frame(anchors.card),
            detail: frame(anchors.detail),
            row: frame(anchors.row),
            scroll: AlbumGridScroll(
                offset: -content.minY,
                visibleHeight: visibleHeight,
                offsets: 0...max(0, content.height - visibleHeight)
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
        case .album(let position, _, _), .placeholder(let position):
            // One view whether the position's page has landed or not: its
            // page hold and its place in the grid go on as the page lands
            // and leaves.
            placed(
                positionContent(cell, width: metrics.cardWidth)
                    .transformAnchorPreference(
                        key: RevealAnchorsKey.self,
                        value: .bounds
                    ) { anchors, bounds in
                        if let seq = revealCardSeq(at: position) {
                            anchors.card = RevealAnchors.Part(
                                seq: seq,
                                bounds: bounds
                            )
                        }
                    }
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
            }
            .onDisappear {
                viewport.remove(cell, columnCount: columnCount)
            }
    }

    /// What a position's slot shows: its album's card once its page has
    /// landed, an empty card until then.
    @ViewBuilder
    private func positionContent(
        _ cell: AlbumGridCell,
        width: CGFloat
    ) -> some View {
        if case .album(let position, let albumId, let sectionId) = cell {
            albumCard(
                albumId,
                position: position,
                sectionId: sectionId,
                width: width
            )
        }
        else {
            AlbumCardPlaceholder(size: width)
                .frame(width: width)
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
    /// Shows what `reveal` names: ask core for the album's index, hold its
    /// page, and have its card report where it is laid out. Each layout that
    /// places the reveal's parts, and each scroll, then decides the next
    /// step from where they are (`stepReveal`). A track's row may already be
    /// laid out in the open detail, and is shown from there.
    ///
    /// The album's page stays held until the reveal ends, which cancels this
    /// task: the rows on screen hold their own pages, and without the hold
    /// the far page the reveal goes to could be dropped before it gets there.
    private func show(_ reveal: PendingAlbumReveal) async {
        stepReveal(reveal)
        if uiStore.pendingAlbumReveal?.seq != reveal.seq {
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
            goToAlbum(of: reveal, at: index)
            await Task.untilCancelled()
        }
    }

    /// Has the card of the album `reveal` names, at list position `index`,
    /// report where it is laid out, and looks for it if it is not in view.
    private func goToAlbum(of reveal: PendingAlbumReveal, at index: Int) {
        // The list moved under the reveal between core placing the album and
        // its page answering: there is no card to go to.
        guard list.idAt(index) == reveal.albumId else {
            albumGridLogger.warning(
                "Album \(reveal.albumId) is no longer at \(index); skipping reveal"
            )
            uiStore.consumeAlbumReveal(seq: reveal.seq)
            return
        }
        viewport.release()
        let sectionId = groupByArtist ? list.section(at: index)?.id : nil
        expandedOccurrence = AlbumOccurrence(
            albumId: reveal.albumId,
            sectionId: sectionId
        )
        revealCard = RevealCard(seq: reveal.seq, position: index)
        // A card in view reports where it is in the layout this sets off.
        if !viewport.shows(.position(index)) {
            stepReveal(reveal)
        }
    }

    /// The pending reveal whose card is the one at `position`.
    private func revealCardSeq(at position: Int) -> Int? {
        guard let revealCard, revealCard.position == position,
            uiStore.pendingAlbumReveal?.seq == revealCard.seq
        else { return nil }
        return revealCard.seq
    }

    /// Takes the pending reveal's next step from the layout that last placed
    /// its parts. The reveal ends once a layout shows what it names, and a
    /// track's row flashes there.
    ///
    /// Its scrolls go to offsets, made at once. A scroll to a slot by its id
    /// goes on placing that slot after later scrolls are asked for, and an
    /// animated change of the scroll position from a slot, where the
    /// person's own scrolling leaves it, to an offset runs from the offset
    /// back to the slot.
    private func stepReveal(_ reveal: PendingAlbumReveal) {
        let card = revealCard.flatMap { $0.seq == reveal.seq ? $0 : nil }
        guard let step = viewport.step(of: reveal, findsCard: card != nil)
        else { return }
        switch step {
        case .shown:
            uiStore.consumeAlbumReveal(seq: reveal.seq)
            if let trackId = reveal.trackId {
                uiStore.flashTrack(trackId, seq: reveal.seq)
            }
        case .scroll(let offset):
            scroll(to: offset)
        case .findCard:
            guard let card, let offset = offset(toCardAt: card.position)
            else { return }
            scroll(to: offset)
        }
    }

    /// Where the lazy grid places the card at `position`, which is not laid
    /// out: its row counted from a slot in view.
    private func offset(toCardAt position: Int) -> CGFloat? {
        let columnCount = viewport.columnCount
        let cells = cells(columnCount: columnCount)
        guard let slot = cells.slot(ofPosition: position) else { return nil }
        return viewport.offset(
            toRow: slot / columnCount,
            of: (cells.count + columnCount - 1) / columnCount,
            rowOf: { shown in
                switch shown {
                case .position(let position):
                    cells.slot(ofPosition: position).map { $0 / columnCount }
                case .detail:
                    nil
                }
            }
        )
    }

    /// Scrolls to `offset` at once, for the reveal.
    private func scroll(to offset: CGFloat) {
        viewport.release()
        var still = Transaction()
        still.disablesAnimations = true
        withTransaction(still) {
            scrollPosition.scrollTo(y: offset)
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
        scrollToTop(slot)
    }

    /// Scrolls `slot` to the top.
    private func scrollToTop(_ slot: AlbumGridSlot) {
        let id: AlbumGridCell.Identity
        switch slot {
        case .position(let position) where position < list.totalCount:
            id = .position(position)
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
                expandedOccurrence = AlbumOccurrence(
                    albumId: albumId,
                    sectionId: sectionId
                )
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
