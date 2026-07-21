import BaeKit
import SwiftUI
import os.log

private let albumGridLogger = Logger.bae("AlbumGridView")
private let albumCardSize: CGFloat = 200
private let gridSpacing: CGFloat = 30
private let loadBatchSize = 50

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
    /// Multi-selection state, owned by `LibraryView`. The grid reads it to render
    /// the selection tint and build bulk-action targets, and mutates it on
    /// modifier clicks / Esc / cmd-A.
    let selection: AlbumGridSelection
    /// Bulk-action closures. Each takes the album ids to act on, in visible grid
    /// order — one album for a plain card, the whole selection for a selected one.
    let onPlay: ([String]) -> Void
    let onAddToQueue: ([String]) -> Void
    let onAddNext: ([String]) -> Void
    let onPin: ([String]) -> Void
    @ViewBuilder
    let expansionContent: (_ albumId: String) -> ExpansionContent

    /// Focus lands on the grid the moment a selection interaction happens, so Esc
    /// (clear) and cmd-A (select all loaded) work immediately after a click.
    @FocusState
    private var gridFocused: Bool

    private static var maxContentWidth: CGFloat {
        1240
    }

    private static var contentPadding: CGFloat {
        16
    }

    var body: some View {
        GeometryReader { geometry in
            let effectiveWidth =
                min(geometry.size.width, Self.maxContentWidth) - Self
                .contentPadding * 2
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
                                await loadBatch(
                                    around: rowIndex * columnCount
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
                    .padding(.horizontal, Self.contentPadding)
                    .padding(.bottom)
                    .frame(maxWidth: Self.maxContentWidth)
                    .frame(maxWidth: .infinity)
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
            await loadBatch(around: index)
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

    /// Load the batch of albums centered on `albumIndex` so its row is
    /// materialized. Shared by lazy row loading and by `revealAlbum`, which
    /// scrolls to a possibly-unfetched album — one definition of the batch
    /// bounds keeps the two from drifting.
    private func loadBatch(around albumIndex: Int) async {
        let first = max(0, albumIndex - loadBatchSize / 2)
        let end = min(first + loadBatchSize, list.totalCount)
        await list.loadRange(offset: first, limit: end - first)
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
            onAddNext: { onAddNext(targets) },
            onPin: { onPin(targets) }
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

private struct AlbumExpansionSlot<ExpansionContent: View>: View {
    let selectedId: String?
    let expansionContent: (String) -> ExpansionContent

    var body: some View {
        ZStack {
            Color.clear.frame(height: 0)
            selectedId.map { id in
                expansionContent(id)
                    .transition(.opacity)
            }
        }
    }
}

// MARK: - Album Card

struct AlbumCardView: View {
    let title: String
    let artistNames: String
    let year: Int32?
    let cover: BridgeImageRef?
    /// The album's detail expansion is open — shown as the accent ring on the art.
    let isExpanded: Bool
    /// The album is part of the multi-selection — shown as a tint behind the card.
    let isSelected: Bool
    let size: CGFloat
    let menu: AlbumCardMenu

    @State
    private var isHovered = false
    @State
    private var showMenu = false

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            albumArt
                .clipShape(RoundedRectangle(cornerRadius: 12))
                .shadow(color: .black.opacity(0.55), radius: 14, y: 9)
                // The open-detail ring sits off the art — a stroke floated
                // outside the cover's edge, not a border eating into it.
                .overlay(
                    RoundedRectangle(cornerRadius: 16)
                        .inset(by: -4.5)
                        .stroke(
                            isExpanded ? Theme.accent : .clear,
                            lineWidth: 3
                        )
                )
                .overlay(alignment: .topTrailing) {
                    CardMenuButton(menu: menu, showMenu: $showMenu)
                        .padding(6)
                        .opacity(isHovered || showMenu ? 1 : 0)
                        .allowsHitTesting(isHovered || showMenu)
                }
                .onHover { isHovered = $0 }
                .padding(.bottom, 10)
            Text(title)
                .font(.system(size: 15, weight: .bold))
                .lineLimit(1)
            Text(artistNames)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            StableOptionalText(
                text: year.map(String.init),
                font: .system(size: 12, weight: .medium),
                foreground: .tertiary,
                lineHeight: 14
            )
        }
        .padding(6)
        // The selection tint stays in the layout tree, toggled by opacity, so a
        // selection change never re-measures the row (layout stability).
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Theme.accentSoft)
                .opacity(isSelected ? 1 : 0)
        )
        .contextMenu {
            AlbumCardMenuItems(menu: menu)
        }
    }

    private var albumArt: some View {
        ImageView(imageRef: cover, pointSize: size)
            .frame(width: size, height: size)
    }
}

// MARK: - Card Menu

/// The bulk-action menu a grid card presents: how many albums it targets and the
/// four actions, each already bound to those targets. Built by the grid from
/// `AlbumGridSelection.orderedTargets`; both the SwiftUI context menu and the
/// AppKit ellipsis menu render from this one definition. Labels switch to the
/// plural form (carrying the count) when more than one album is targeted.
struct AlbumCardMenu {
    let targetCount: Int
    let onPlay: () -> Void
    let onAddToQueue: () -> Void
    let onAddNext: () -> Void
    let onPin: () -> Void

    var playLabel: String {
        targetCount > 1
            ? String(localized: "Play \(targetCount) Albums")
            : String(localized: "Play")
    }

    var addToQueueLabel: String {
        targetCount > 1
            ? String(localized: "Add \(targetCount) Albums to Queue")
            : String(localized: "Add to Queue")
    }

    var addNextLabel: String {
        targetCount > 1
            ? String(localized: "Add \(targetCount) Albums Next")
            : String(localized: "Add Next")
    }

    var pinLabel: String {
        targetCount > 1
            ? String(localized: "Pin \(targetCount) Albums for Offline")
            : String(localized: "Pin for offline")
    }
}

/// The SwiftUI rendering of an `AlbumCardMenu` — the `.contextMenu` items.
private struct AlbumCardMenuItems: View {
    let menu: AlbumCardMenu

    var body: some View {
        Button(menu.playLabel) { menu.onPlay() }
        Button(menu.addToQueueLabel) { menu.onAddToQueue() }
        Button(menu.addNextLabel) { menu.onAddNext() }
        Button(menu.pinLabel) { menu.onPin() }
    }
}

// MARK: - Card Overlay Button

private struct CardMenuButton: View {
    let menu: AlbumCardMenu
    @Binding
    var showMenu: Bool
    @State
    private var isHovered = false

    var body: some View {
        Button(action: presentMenu) {
            Image(systemName: "ellipsis")
                .font(.system(size: 13, weight: .semibold))
                .foregroundColor(.white)
                .frame(width: 30, height: 30)
                .background(
                    isHovered ? Color.accentColor : Color.black.opacity(0.4)
                )
                .clipShape(Circle())
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }

    private func presentMenu() {
        showMenu = true
        let nsMenu = NSMenu()
        nsMenu.addItem(MenuItem(title: menu.playLabel, handler: menu.onPlay))
        nsMenu.addItem(NSMenuItem.separator())
        nsMenu.addItem(
            MenuItem(title: menu.addToQueueLabel, handler: menu.onAddToQueue)
        )
        nsMenu.addItem(
            MenuItem(title: menu.addNextLabel, handler: menu.onAddNext)
        )
        nsMenu.addItem(MenuItem(title: menu.pinLabel, handler: menu.onPin))

        nsMenu.popUp(
            positioning: nil,
            at: NSEvent.mouseLocation,
            in: nil
        )
        showMenu = false
    }
}

private class MenuItem: NSMenuItem {
    private let handler: () -> Void

    init(title: String, handler: @escaping () -> Void) {
        self.handler = handler
        super.init(title: title, action: #selector(fire), keyEquivalent: "")
        target = self
    }

    @available(*, unavailable)
    required init(coder _: NSCoder) {
        fatalError()
    }

    @objc
    private func fire() {
        handler()
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
                selection: AlbumGridSelection(),
                onPlay: { _ in },
                onAddToQueue: { _ in },
                onAddNext: { _ in },
                onPin: { _ in },
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

    #Preview("Album Card") {
        let album = PreviewData.albums[0]
        let selected = PreviewData.albums[3]
        let menu = AlbumCardMenu(
            targetCount: 1,
            onPlay: {},
            onAddToQueue: {},
            onAddNext: {},
            onPin: {}
        )
        HStack(spacing: 20) {
            AlbumCardView(
                title: album.title,
                artistNames: album.artistNames,
                year: album.year,
                cover: nil,
                isExpanded: false,
                isSelected: false,
                size: albumCardSize,
                menu: menu,
            )
            AlbumCardView(
                title: selected.title,
                artistNames: selected.artistNames,
                year: selected.year,
                cover: nil,
                isExpanded: false,
                isSelected: true,
                size: albumCardSize,
                menu: menu,
            )
        }
        .padding()
        .environment(MediaPaths.stub)
    }
#endif
