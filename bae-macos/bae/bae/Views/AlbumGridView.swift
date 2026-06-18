import SwiftUI

private let albumCardSize: CGFloat = 180
private let gridSpacing: CGFloat = 24
private let loadBatchSize = 50

struct AlbumGridView<ExpansionContent: View>: View {
    @Environment(UiStore.self)
    private var uiStore
    @Environment(LibraryStore.self)
    private var libraryStore
    let list: AlbumList
    @Binding
    var selectedAlbumId: String?
    @Binding
    var sortCriteria: [BridgeSortCriterion]
    let availableFields: [BridgeSortField]
    let onPlay: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onAddNext: (String) -> Void
    let headerTitle: String
    @ViewBuilder
    let expansionContent: (_ albumId: String) -> ExpansionContent

    private static var maxContentWidth: CGFloat {
        1200
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
                    VStack(alignment: .leading, spacing: 0) {
                        libraryHeader
                            .padding(.horizontal, Self.contentPadding)
                            .padding(.top, 40)
                            .padding(.bottom, 20)
                        LazyVStack(alignment: .leading, spacing: 28) {
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
                                                    coverPath: summary
                                                        .coverPath,
                                                    isSelected: selectedAlbumId
                                                        == summary.id,
                                                    size: cardWidth,
                                                    onPlay: {
                                                        onPlay(
                                                            summary
                                                                .primaryReleaseId
                                                        )
                                                    },
                                                    onAddToQueue: {
                                                        onAddToQueue(
                                                            summary
                                                                .primaryReleaseId
                                                        )
                                                    },
                                                    onAddNext: {
                                                        onAddNext(
                                                            summary
                                                                .primaryReleaseId
                                                        )
                                                    },
                                                )
                                                .id(summary.id)
                                                .frame(width: cardWidth)
                                                .draggable(summary.id)
                                                .onTapGesture {
                                                    withAnimation(
                                                        .spring(
                                                            response: 0.3,
                                                            dampingFraction:
                                                                0.85
                                                        )
                                                    ) {
                                                        if uiStore
                                                            .selectedAlbumId
                                                            == summary.id
                                                        {
                                                            uiStore
                                                                .closeAlbumDetail()
                                                        }
                                                        else {
                                                            uiStore.selectAlbum(
                                                                summary.id
                                                            )
                                                        }
                                                    }
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
                                    let firstAlbum = max(
                                        0,
                                        rowIndex * columnCount - loadBatchSize
                                            / 2
                                    )
                                    let batchEnd = min(
                                        firstAlbum + loadBatchSize,
                                        list.totalCount
                                    )
                                    await list.loadRange(
                                        offset: firstAlbum,
                                        limit: batchEnd - firstAlbum
                                    )
                                }
                                if let selectedId = selectedAlbumId,
                                    (0..<columnCount)
                                        .contains(where: { col in
                                            let idx =
                                                rowIndex * columnCount + col
                                            return idx < list.totalCount
                                                && list.idAt(idx) == selectedId
                                        })
                                {
                                    expansionContent(selectedId)
                                        .transition(.opacity)
                                }
                            }
                        }
                        .padding(.horizontal, Self.contentPadding)
                        .padding(.bottom)
                    }
                    .frame(maxWidth: Self.maxContentWidth)
                    .frame(maxWidth: .infinity)
                }
                .onReceive(uiStore.navigationSubject) { command in
                    guard let albumIndex = list.position(of: command.albumId),
                        columnCount > 0
                    else {
                        return
                    }

                    let rowIndex = albumIndex / columnCount
                    Task { @MainActor in
                        try? await Task.sleep(for: .milliseconds(50))
                        withAnimation(.easeInOut(duration: 0.3)) {
                            scrollProxy.scrollTo(rowIndex, anchor: .top)
                        }
                    }
                }
            }
        }
        .background(Theme.background)
    }

    private var libraryHeader: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(headerTitle)
                .font(.system(size: 36, weight: .bold))
            Spacer()
            sortControls
        }
    }

    private var usedFields: Set<BridgeSortField> {
        Set(sortCriteria.map(\.field))
    }

    private var sortControls: some View {
        HStack(spacing: 4) {
            ForEach(sortCriteria.indices, id: \.self) { index in
                SortCriterionChip(
                    criterion: $sortCriteria[index],
                    choosableFields: availableFields.filter {
                        $0 == sortCriteria[index].field
                            || !usedFields.contains($0)
                    },
                    canRemove: sortCriteria.count > 1,
                    onRemove: { sortCriteria.remove(at: index) },
                )
            }
            let unused = availableFields.filter { !usedFields.contains($0) }
            if !unused.isEmpty {
                Menu {
                    ForEach(unused, id: \.self) { field in
                        Button(field.displayName) {
                            sortCriteria.append(
                                BridgeSortCriterion(
                                    field: field,
                                    direction: .ascending
                                )
                            )
                        }
                    }
                } label: {
                    Image(systemName: "plus")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                .help("Add sort criterion")
            }
        }
    }
}

// MARK: - Sort Criterion Chip

private struct SortCriterionChip: View {
    @Binding
    var criterion: BridgeSortCriterion
    let choosableFields: [BridgeSortField]
    let canRemove: Bool
    let onRemove: () -> Void

    var body: some View {
        Menu {
            Button {
                criterion.direction =
                    criterion.direction == .ascending ? .descending : .ascending
            } label: {
                Label(
                    criterion.direction == .ascending
                        ? "Sort Descending" : "Sort Ascending",
                    systemImage: criterion.direction == .ascending
                        ? "arrow.down" : "arrow.up",
                )
            }
            if canRemove {
                Button(role: .destructive, action: onRemove) {
                    Label("Remove", systemImage: "xmark.circle")
                }
            }
            Divider()
            ForEach(choosableFields, id: \.self) { field in
                Button {
                    criterion.field = field
                } label: {
                    HStack {
                        Text(field.displayName)
                        if criterion.field == field {
                            Image(systemName: "checkmark")
                        }
                    }
                }
            }
        } label: {
            HStack(spacing: 2) {
                Text(criterion.field.displayName)
                Image(
                    systemName: criterion.direction == .ascending
                        ? "arrow.up" : "arrow.down"
                )
            }
            .font(.callout)
            .foregroundStyle(.secondary)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }
}

// MARK: - Album Card

struct AlbumCardView: View {
    let title: String
    let artistNames: String
    let year: Int32?
    let coverPath: String?
    let isSelected: Bool
    let size: CGFloat
    let onPlay: () -> Void
    let onAddToQueue: () -> Void
    let onAddNext: () -> Void

    @State
    private var isHovered = false
    @State
    private var showMenu = false

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            albumArt
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .strokeBorder(
                            Color.accentColor,
                            lineWidth: isSelected ? 3 : 0,
                        ),
                )
                .overlay(alignment: .topTrailing) {
                    CardMenuButton(
                        onPlay: onPlay,
                        onAddToQueue: onAddToQueue,
                        onAddNext: onAddNext,
                        showMenu: $showMenu
                    )
                    .padding(6)
                    .opacity(isHovered || showMenu ? 1 : 0)
                    .allowsHitTesting(isHovered || showMenu)
                }
                .onHover { isHovered = $0 }
                .padding(.bottom, 6)
            Text(title)
                .font(.body)
                .fontWeight(.medium)
                .lineLimit(1)
            Text(artistNames)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            if let year {
                Text(String(year))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .contextMenu {
            Button("Play") { onPlay() }
            Button("Add to Queue") { onAddToQueue() }
            Button("Add Next") { onAddNext() }
        }
    }

    private var albumArt: some View {
        ImageView(localPath: coverPath, pointSize: size)
            .frame(width: size, height: size)
    }
}

// MARK: - Card Overlay Button

private struct CardMenuButton: View {
    let onPlay: () -> Void
    let onAddToQueue: () -> Void
    let onAddNext: () -> Void
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
        let menu = NSMenu()
        let playItem = MenuItem(title: "Play") { onPlay() }
        menu.addItem(playItem)
        menu.addItem(NSMenuItem.separator())
        let queueItem = MenuItem(title: "Add to Queue") { onAddToQueue() }
        menu.addItem(queueItem)
        let nextItem = MenuItem(title: "Add Next") { onAddNext() }
        menu.addItem(nextItem)

        menu.popUp(
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
        @State
        private var selectedAlbumId: String? = "a-04"
        @State
        private var sortCriteria: [BridgeSortCriterion] = [
            BridgeSortCriterion(field: .dateAdded, direction: .descending)
        ]

        var body: some View {
            let list = AlbumList.preview(
                albums: PreviewData.albums,
                sort: sortCriteria,
                store: store
            )
            AlbumGridView(
                list: list,
                selectedAlbumId: $selectedAlbumId,
                sortCriteria: $sortCriteria,
                availableFields: BridgeSortField.allCases,
                onPlay: { _ in },
                onAddToQueue: { _ in },
                onAddNext: { _ in },
                headerTitle: "Library",
            ) { albumId in
                AlbumDetailView(albumId: albumId)
            }
            .frame(width: width, height: height)
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
        HStack(spacing: 20) {
            AlbumCardView(
                title: album.title,
                artistNames: album.artistNames,
                year: album.year,
                coverPath: nil,
                isSelected: false,
                size: albumCardSize,
                onPlay: {},
                onAddToQueue: {},
                onAddNext: {},
            )
            AlbumCardView(
                title: selected.title,
                artistNames: selected.artistNames,
                year: selected.year,
                coverPath: nil,
                isSelected: true,
                size: albumCardSize,
                onPlay: {},
                onAddToQueue: {},
                onAddNext: {},
            )
        }
        .padding()
        .environment(MediaPaths.stub)
    }
#endif
