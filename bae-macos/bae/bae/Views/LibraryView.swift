import BaeKit
import SwiftUI

private let listLoadBatchSize = 50

private enum ComposerPaneDetail {
    case empty
    case composer(BridgeComposerDetail, work: BridgeWorkDetail?)
    case work(BridgeWorkDetail)
}

struct LibraryView: View {
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(Library.self)
    var library
    @Environment(Downloads.self)
    var downloads
    @Environment(LibraryStore.self)
    var libraryStore
    @Environment(UiStore.self)
    var uiStore
    @Environment(LibraryBrowseSession.self)
    var session

    /// Detail payloads derived from `session.detailSelection` /
    /// `session.selectedArtistId`. View-local by design (see
    /// `LibraryBrowseSession`'s doc comment): a remount re-fetches them cheaply
    /// rather than keeping them warm, while the selections themselves persist.
    @State
    private var composerPaneDetail: ComposerPaneDetail = .empty
    @State
    private var artistDetail: BridgeArtistDetail?
    /// Collapse kinematics for the header, fed by the panes' scroll reports
    /// (`reportsHeaderScroll`). The header scrubs between its full and
    /// compact metrics off `headerCollapse.progress`, reclaiming the
    /// vertical room the full-size heading occupies at rest.
    @State
    private var headerCollapse = HeaderCollapse()

    var body: some View {
        VStack(spacing: 0) {
            libraryHeader
            Group {
                switch uiStore.libraryBrowserMode {
                case .albums:
                    albumContent
                case .composers:
                    composerContent
                case .artists:
                    artistContent
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .environment(headerCollapse)
        }
        // The page sits on a faint radial lift toward the top-trailing corner
        // rather than a flat fill, so the header band reads as its own region.
        // One surface step above the base — any lighter reads as a spotlight.
        .background(
            RadialGradient(
                colors: [Theme.surface, Theme.background],
                center: UnitPoint(x: 0.8, y: -0.1),
                startRadius: 0,
                endRadius: 900
            )
        )
        .task(id: uiStore.libraryBrowserMode) {
            switch uiStore.libraryBrowserMode {
            case .albums:
                break
            case .composers:
                await session.composers.ensureLoaded()
            case .artists:
                await session.artists.ensureLoaded()
            }
        }
        .task(id: session.detailSelection.composerId) {
            await loadComposerDetail()
        }
        .task(id: session.detailSelection) {
            await loadWorkDetail()
        }
        .task(id: session.selectedArtistId) {
            await loadArtistDetail()
        }
        .task(id: uiStore.pendingLibraryNavigation?.seq) {
            guard let request = uiStore.pendingLibraryNavigation else {
                return
            }
            applyLibraryNavigation(request.target)
            uiStore.consumeLibraryNavigation(seq: request.seq)
        }
        // Opening a detail expansion (plain click, search, external navigation)
        // clears the multi-selection — the same semantics as a plain grid click.
        .onChange(of: uiStore.selectedAlbumId) { _, selected in
            if selected != nil {
                session.albumSelection.clear()
            }
        }
        // The album-list invalidation is coarse (no per-id removal event), so
        // after a shape change prune any selected album that no longer resolves —
        // e.g. one deleted on another device. The selection is small; one
        // authoritative index lookup per id, cancellable between ids.
        .task(id: session.albums.list?.loadEpoch) {
            await pruneDeletedSelection()
        }
        // Mirror the library-wide album count into the store: the menu bar's
        // Shuffle Library item (MainAppMenuCommands) disables at zero, and the
        // menu has no album list of its own to ask. `initial: true` publishes
        // a count that was already loaded when this view (re)mounts.
        .onChange(of: session.albums.list?.totalCount, initial: true) {
            _,
            count in
            if let count {
                libraryStore.setAlbumTotal(count)
            }
        }
    }
}

extension LibraryView {
    private var queueActions: QueueActions {
        QueueActions(library: library, queue: queue, uiStore: uiStore)
    }

    /// The primary release id of each album, in the album order given. The grid's
    /// bulk Play and Pin act on releases; a selected album is loaded (its summary
    /// is interned), so the lookup resolves.
    private func primaryReleaseIds(for albumIds: [String]) -> [String] {
        albumIds.compactMap {
            libraryStore.albumSummaries[$0]?.primaryReleaseId
        }
    }

    /// Drop any selected album that no longer resolves under the current sort —
    /// the authoritative `getAlbumIndex` returning nil means it was deleted. Runs
    /// off the album-list epoch; checks cancellation between ids so a newer epoch
    /// supersedes it cleanly.
    private func pruneDeletedSelection() async {
        guard !session.albumSelection.isEmpty else {
            return
        }
        var missing: [String] = []
        for id in session.albumSelection.selectedIds {
            if Task.isCancelled {
                return
            }
            do {
                if try await library.getAlbumIndex(
                    session.albums.sortCriteria,
                    id
                )
                    == nil
                {
                    missing.append(id)
                }
            }
            catch {
                // A lookup failure isn't evidence the album is gone; leave the
                // selection untouched rather than prune on a transient error.
                return
            }
        }
        if !missing.isEmpty {
            session.albumSelection.remove(missing)
        }
    }

    /// Pinned above the content, fixed across mode switches. The heading *is*
    /// the mode switcher; the trailing controls are mode-specific.
    private var libraryHeader: some View {
        LibraryHeader(collapseProgress: headerCollapse.progress) {
            switch uiStore.libraryBrowserMode {
            case .albums:
                sortControls(session.albums)
            case .composers:
                sortControls(session.composers)
            case .artists:
                sortControls(session.artists)
            }
        }
    }

    private func sortControls<
        Row: Identifiable & Sendable,
        Criterion: SortCriterionRepresentable
    >(
        _ slot: BrowseListSlot<Row, Criterion>
    ) -> some View
    where Row.ID: Sendable, Criterion.Field: SortCriterionFieldCodable {
        SortCriteriaRow(
            criteria: Binding(
                get: { slot.sortCriteria },
                set: { slot.setSortCriteria($0) }
            )
        )
    }

    private var albumContent: some View {
        Group {
            if let albumList = session.albums.list {
                if let error = albumList.initialLoadError {
                    LoadFailureView(line: error.line) {
                        Task { await albumList.loadInitial() }
                    }
                }
                else if albumList.totalCount == 0 {
                    ContentUnavailableView(
                        "No albums",
                        systemImage: "square.stack",
                        description: Text("Import some music to get started"),
                    )
                }
                else {
                    AlbumGridView(
                        list: albumList,
                        sortCriteria: session.albums.sortCriteria,
                        selection: session.albumSelection,
                        onPlay: { albumIds in
                            playback.playReleases(
                                primaryReleaseIds(for: albumIds)
                            )
                        },
                        onAddToQueue: { albumIds in
                            queueActions.addToQueue(albumIds)
                        },
                        onAddNext: { albumIds in
                            queueActions.addNext(albumIds)
                        },
                        onPin: { albumIds in
                            let releaseIds = primaryReleaseIds(for: albumIds)
                            Task { await downloads.queuePins(releaseIds) }
                        },
                    ) { albumId in
                        AlbumDetailView(albumId: albumId)
                    }
                }
            }
            else {
                ProgressView()
            }
        }
    }

    private var composerContent: some View {
        Group {
            if let composerList = session.composers.list {
                if let error = composerList.initialLoadError {
                    LoadFailureView(line: error.line) {
                        Task { await composerList.loadInitial() }
                    }
                }
                else if composerList.totalCount == 0 {
                    ContentUnavailableView(
                        "No composers",
                        systemImage: "person.wave.2",
                        description: Text("Import some music to get started"),
                    )
                }
                else {
                    HSplitView {
                        BrowseList(list: composerList) { index in
                            composerRow(at: index, list: composerList)
                        }
                        .frame(minWidth: 260, idealWidth: 320)
                        composerDetailView
                            .frame(minWidth: 420)
                    }
                }
            }
            else {
                ProgressView()
            }
        }
    }

    private var artistContent: some View {
        Group {
            if let artistList = session.artists.list {
                if let error = artistList.initialLoadError {
                    LoadFailureView(line: error.line) {
                        Task { await artistList.loadInitial() }
                    }
                }
                else if artistList.totalCount == 0 {
                    ContentUnavailableView(
                        "No artists",
                        systemImage: "music.mic",
                        description: Text("Import some music to get started"),
                    )
                }
                else {
                    HSplitView {
                        BrowseList(list: artistList) { index in
                            artistRow(at: index, list: artistList)
                        }
                        .frame(minWidth: 260, idealWidth: 320)
                        artistDetailView
                            .frame(minWidth: 420)
                    }
                }
            }
            else {
                ProgressView()
            }
        }
    }

    private func applyLibraryNavigation(_ target: LibraryNavigationTarget) {
        switch target {
        case .composer(let artistId):
            session.selectComposer(artistId)
            composerPaneDetail = .empty
        case .work(let workId):
            session.selectWork(workId)
            composerPaneDetail = .empty
        }
    }

    private func artistRow(at index: Int, list: ArtistList) -> some View {
        let id = list.idAt(index)
        return BrowseListRow(
            id: id,
            isSelected: id != nil && session.selectedArtistId == id,
            summaries: \.artistSummaries,
            select: { id in
                session.selectArtist(id)
                artistDetail = nil
            }
        )
    }

    private func composerRow(at index: Int, list: ComposerList) -> some View {
        let id = list.idAt(index)
        return BrowseListRow(
            id: id,
            isSelected: id != nil && session.detailSelection.composerId == id,
            summaries: \.composerSummaries,
            select: { id in
                session.selectComposer(id)
                composerPaneDetail = .empty
            }
        )
    }

    private var composerDetailView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if case .composer(let composerDetail, let loadedWorkDetail) =
                    composerPaneDetail
                {
                    BrowseDetailHeader(summary: composerDetail.composer)
                    if !composerDetail.workGroups.isEmpty {
                        SectionHeader(title: String(localized: "Works"))
                        ForEach(composerDetail.workGroups) { group in
                            ComposerWorkGroupView(group: group) { workId in
                                if let artistId = session.detailSelection
                                    .composerId
                                {
                                    session.selectComposerWork(
                                        artistId: artistId,
                                        workId: workId
                                    )
                                    composerPaneDetail = .composer(
                                        composerDetail,
                                        work: nil
                                    )
                                }
                            }
                        }
                    }
                    if !composerDetail.unlinkedReleaseRoles.isEmpty {
                        SectionHeader(title: String(localized: "Credits"))
                        ForEach(
                            composerDetail.unlinkedReleaseRoles,
                            id: \.releaseId
                        ) { role in
                            CreditRow(
                                title: role.albumTitle,
                                subtitle: role.sourceCredit
                            )
                        }
                    }
                    if !composerDetail.unlinkedTrackRoles.isEmpty {
                        ForEach(
                            composerDetail.unlinkedTrackRoles,
                            id: \.trackId
                        ) { role in
                            CreditRow(
                                title: role.trackTitle,
                                subtitle: role.albumTitle
                            )
                        }
                    }
                    if let loadedWorkDetail {
                        Divider()
                        WorkDetailView(
                            detail: loadedWorkDetail,
                            openWork: { workId in
                                if case .composer(let artistId, _) =
                                    session.detailSelection
                                {
                                    session.selectComposerWork(
                                        artistId: artistId,
                                        workId: workId
                                    )
                                    composerPaneDetail = .composer(
                                        composerDetail,
                                        work: nil
                                    )
                                }
                            },
                            openAlbum: { albumId, releaseId in
                                uiStore.navigateToAlbum(
                                    albumId,
                                    releaseId: releaseId
                                )
                            }
                        )
                    }
                }
                if case .work(let loadedWorkDetail) = composerPaneDetail {
                    WorkDetailView(
                        detail: loadedWorkDetail,
                        openWork: { workId in
                            session.selectWork(workId)
                            composerPaneDetail = .empty
                        },
                        openAlbum: { albumId, releaseId in
                            uiStore.navigateToAlbum(
                                albumId,
                                releaseId: releaseId
                            )
                        }
                    )
                }
                if session.detailSelection == .none {
                    ContentUnavailableView(
                        "Composers",
                        systemImage: "person.wave.2"
                    )
                }
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .reportsHeaderScroll(id: "composerDetail")
        .background(Theme.surface)
    }

    private var artistDetailView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let artistDetail {
                    BrowseDetailHeader(summary: artistDetail.artist)
                    LazyVGrid(
                        columns: [
                            GridItem(.adaptive(minimum: 140), spacing: 12)
                        ],
                        alignment: .leading,
                        spacing: 12
                    ) {
                        ForEach(artistDetail.albums) { album in
                            ArtistAlbumCard(album: album) {
                                uiStore.navigateToAlbum(album.id)
                            }
                        }
                    }
                }
                else if session.selectedArtistId == nil {
                    ContentUnavailableView(
                        "Artists",
                        systemImage: "music.mic"
                    )
                }
                else {
                    ProgressView()
                }
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .reportsHeaderScroll(id: "artistDetail")
        .background(Theme.surface)
    }

    private func loadComposerDetail() async {
        guard let selectedComposerId = session.detailSelection.composerId
        else {
            return
        }
        do {
            let getComposerDetail = library.getComposerDetail
            let detail = try await getComposerDetail(selectedComposerId)
            guard !Task.isCancelled else {
                return
            }
            guard let detail else {
                uiStore.showError(
                    DisplayError(
                        line: String(localized: "Composer detail not found")
                    )
                )
                return
            }
            composerPaneDetail = .composer(detail, work: nil)
            if case .composer(let artistId, nil) = session.detailSelection,
                artistId == selectedComposerId,
                let defaultWorkId = detail.defaultWorkId
            {
                session.selectComposerWork(
                    artistId: selectedComposerId,
                    workId: defaultWorkId
                )
            }
        }
        catch {
            uiStore.showError(error)
        }
    }

    private func loadArtistDetail() async {
        guard let requestedArtistId = session.selectedArtistId else {
            return
        }
        do {
            let getArtistDetail = library.getArtistDetail
            let detail = try await getArtistDetail(requestedArtistId)
            guard !Task.isCancelled else {
                return
            }
            guard session.selectedArtistId == requestedArtistId else {
                return
            }
            guard let detail else {
                uiStore.showError(
                    DisplayError(
                        line: String(localized: "Artist detail not found")
                    )
                )
                return
            }
            artistDetail = detail
        }
        catch {
            uiStore.showError(error)
        }
    }

    private func loadWorkDetail() async {
        let selectedDetail = session.detailSelection
        guard let selectedWorkId = selectedDetail.workId else {
            return
        }
        do {
            let getWorkDetail = library.getWorkDetail
            let detail = try await getWorkDetail(selectedWorkId)
            guard !Task.isCancelled else {
                return
            }
            guard session.detailSelection == selectedDetail else {
                return
            }
            guard let detail else {
                uiStore.showError(
                    DisplayError(
                        line: String(localized: "Work detail not found")
                    )
                )
                return
            }
            switch selectedDetail {
            case .composer(let artistId, .some):
                guard
                    case .composer(let composerDetail, _) = composerPaneDetail,
                    composerDetail.composer.artistId == artistId
                else {
                    uiStore.showError(
                        DisplayError(
                            line: String(localized: "Composer detail not found")
                        )
                    )
                    return
                }
                composerPaneDetail = .composer(composerDetail, work: detail)
            case .work:
                composerPaneDetail = .work(detail)
            case .none, .composer(_, .none):
                return
            }
        }
        catch {
            uiStore.showError(error)
        }
    }
}

extension BridgeComposerWorkGroup: Identifiable {}

/// The fields the composer and artist browse UIs render for a summary: the
/// list row (36 pt) and the detail-pane header (72 pt) both show an image, a
/// name, and a count line.
private protocol BrowseSummaryDisplay {
    var image: BridgeImageRef? { get }
    var name: String { get }
    var countText: String { get }
}

extension BridgeComposerSummary: BrowseSummaryDisplay {
    fileprivate var countText: String {
        "\(workCount) \(String(localized: "Works"))"
    }
}

extension BridgeArtistSummary: BrowseSummaryDisplay {
    fileprivate var countText: String {
        "\(albumCount) \(String(localized: "Albums"))"
    }
}

/// The composer/artist browser's master list: virtualized rows over a
/// `PaginatedList`, each visible row batch-loading the window around it.
private struct BrowseList<Row: Identifiable & Sendable, RowView: View>: View
where Row.ID: Sendable {
    let list: PaginatedList<Row>
    let row: (Int) -> RowView

    var body: some View {
        List {
            ForEach(0..<list.totalCount, id: \.self) { index in
                row(index)
                    .task(id: RowLoadID(epoch: list.loadEpoch, index: index)) {
                        let first = max(0, index - listLoadBatchSize / 2)
                        let end = min(
                            first + listLoadBatchSize,
                            list.totalCount
                        )
                        await list.loadRange(
                            offset: first,
                            limit: end - first
                        )
                    }
            }
        }
        .scrollContentBackground(.hidden)
        .reportsHeaderScroll(id: "browseList")
        .background(Theme.background)
    }
}

private struct BrowseListRow<Summary: BrowseSummaryDisplay>: View {
    @Environment(LibraryStore.self)
    private var libraryStore

    let id: String?
    let isSelected: Bool
    let summaries: KeyPath<LibraryStore, [String: Summary]>
    let select: (String) -> Void

    var body: some View {
        let summary = id.flatMap { libraryStore[keyPath: summaries][$0] }
        Button(action: {
            guard let id else {
                return
            }
            select(id)
        }) {
            ZStack(alignment: .leading) {
                SummaryRowPlaceholder()
                    .opacity(summary == nil ? 1 : 0)
                    .allowsHitTesting(summary == nil)
                BrowseSummaryRow(summary: summary, isSelected: isSelected)
                    .opacity(summary == nil ? 0 : 1)
                    .allowsHitTesting(summary != nil)
            }
        }
        .buttonStyle(.plain)
        .disabled(summary == nil)
    }
}

private struct BrowseSummaryRow<Summary: BrowseSummaryDisplay>: View {
    let summary: Summary?
    let isSelected: Bool

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary?.image, pointSize: 36)
                .frame(width: 36, height: 36)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 3) {
                OptionalLineText(
                    text: summary?.name,
                    font: .body,
                    foreground: .primary
                )
                OptionalLineText(
                    text: summary?.countText,
                    font: .caption,
                    foreground: .secondary
                )
            }
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 6)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(isSelected ? Color.accentColor.opacity(0.18) : .clear)
        )
    }
}

private struct OptionalLineText: View {
    let text: String?
    let font: Font
    let foreground: StableOptionalTextForeground

    var body: some View {
        StableOptionalText(
            text: text,
            font: font,
            foreground: foreground,
            lineHeight: lineHeight,
            lineLimit: 1
        )
    }

    private var lineHeight: CGFloat {
        switch font {
        case .caption:
            12
        default:
            16
        }
    }
}

private struct SummaryRowPlaceholder: View {
    var body: some View {
        HStack(spacing: 12) {
            RoundedRectangle(cornerRadius: 6)
                .fill(.secondary.opacity(0.15))
                .frame(width: 36, height: 36)
            VStack(alignment: .leading, spacing: 6) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.15))
                    .frame(width: 140, height: 12)
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.12))
                    .frame(width: 80, height: 10)
            }
        }
        .padding(.vertical, 4)
    }
}

private struct BrowseDetailHeader<Summary: BrowseSummaryDisplay>: View {
    let summary: Summary

    var body: some View {
        HStack(spacing: 16) {
            ImageView(imageRef: summary.image, pointSize: 72)
                .frame(width: 72, height: 72)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            VStack(alignment: .leading, spacing: 6) {
                Text(summary.name)
                    .font(.title.bold())
                    .lineLimit(2)
                Text(summary.countText)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct ArtistAlbumCard: View {
    let album: BridgeAlbum
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 6) {
                ImageView(imageRef: album.cover, pointSize: 140)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                Text(album.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                StableOptionalText(
                    text: album.year.map(String.init),
                    font: .caption,
                    foreground: .secondary,
                    lineHeight: 12,
                    lineLimit: 1
                )
            }
        }
        .buttonStyle(.plain)
    }
}

private struct WorkSummaryRow: View {
    let summary: BridgeWorkSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary.representativeCover, pointSize: 42)
                .frame(width: 42, height: 42)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 3) {
                Text(summary.title)
                    .font(.body)
                    .lineLimit(1)
                OptionalLineText(
                    text: summary.composerNames,
                    font: .caption,
                    foreground: .secondary
                )
            }
            Spacer()
        }
        .padding(.vertical, 4)
    }
}

private struct ComposerWorkGroupView: View {
    let group: BridgeComposerWorkGroup
    let openWork: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(parentRows, id: \.workId) { parent in
                Button(action: { openWork(parent.workId) }) {
                    WorkSummaryRow(summary: parent)
                }
                .buttonStyle(.plain)
            }
            VStack(alignment: .leading, spacing: 6) {
                ForEach(group.works, id: \.workId) { work in
                    Button(action: { openWork(work.workId) }) {
                        WorkSummaryRow(summary: work)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.leading, group.parent == nil ? 0 : 18)
        }
    }

    private var parentRows: [BridgeWorkSummary] {
        guard let parent = group.parent else {
            return []
        }
        return [parent]
    }
}

private struct SectionHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.headline)
            .padding(.top, 4)
    }
}

private struct CreditRow: View {
    let title: String
    let subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.body)
                .lineLimit(1)
            StableOptionalText(
                text: subtitle,
                font: .caption,
                foreground: .secondary,
                lineHeight: 12,
                lineLimit: 1
            )
        }
        .padding(.vertical, 4)
    }
}

private struct WorkDetailView: View {
    let detail: BridgeWorkDetail
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(detail.work.title)
                .font(.title2.bold())
                .lineLimit(2)
            if !detail.childWorks.isEmpty {
                SectionHeader(title: String(localized: "Works"))
                ForEach(detail.childWorks, id: \.workId) { work in
                    Button(action: { openWork(work.workId) }) {
                        WorkSummaryRow(summary: work)
                    }
                    .buttonStyle(.plain)
                }
            }
            if !detail.releases.isEmpty {
                SectionHeader(title: String(localized: "Releases"))
                ForEach(detail.releases, id: \.releaseId) { release in
                    Button(action: {
                        openAlbum(release.albumId, release.releaseId)
                    }) {
                        HStack(spacing: 12) {
                            ImageView(imageRef: release.cover, pointSize: 42)
                                .frame(width: 42, height: 42)
                                .clipShape(RoundedRectangle(cornerRadius: 6))
                            VStack(alignment: .leading, spacing: 3) {
                                Text(release.albumTitle)
                                    .font(.body)
                                    .lineLimit(1)
                                StableOptionalText(
                                    text: workReleaseMetadata(release),
                                    font: .caption,
                                    foreground: .secondary,
                                    lineHeight: 12,
                                    lineLimit: 1
                                )
                            }
                            Spacer()
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
            if !detail.tracks.isEmpty {
                SectionHeader(title: String(localized: "Recordings"))
                ForEach(detail.tracks, id: \.trackId) { track in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(track.trackTitle)
                            .font(.body)
                            .lineLimit(1)
                        Text(track.albumTitle)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
    }

    private func workReleaseMetadata(
        _ release: BridgeWorkReleaseSummary
    ) -> String {
        precondition(
            !release.displayName.isEmpty,
            "work release display name is empty for \(release.releaseId)"
        )
        if let format = release.format, !format.isEmpty {
            return "\(release.displayName) \u{00B7} \(format)"
        }
        return release.displayName
    }
}

#if DEBUG
    extension LibraryView {
        /// A `Library` whose album/composer counts and pages are all empty,
        /// driving the real `loadInitial()` → `totalCount == 0` path so the
        /// previews below hit the actual empty-state branches in
        /// `albumContent`/`composerContent` rather than a hand-built stand-in.
        fileprivate static var emptyLibrary: Library {
            Library(
                getAlbumCount: { 0 },
                getAlbumPage: { _, _, _ in [] },
                getComposerCount: { 0 },
                getComposerPage: { _, _, _ in [] },
                getArtistCount: { 0 },
                getArtistPage: { _, _, _ in [] },
            )
        }
    }

    #Preview("Albums \u{2014} Empty") {
        let uiStore = UiStore()
        let libraryStore = LibraryStore()
        let session = LibraryBrowseSession(
            library: LibraryView.emptyLibrary,
            projectionRegistry: ProjectionRegistry(),
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        return LibraryView()
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(LibraryView.emptyLibrary)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .frame(width: 1100, height: 700)
            .windowBackground()
    }

    #Preview("Composers \u{2014} Empty") {
        let uiStore = UiStore()
        uiStore.setLibraryBrowserMode(.composers)
        let libraryStore = LibraryStore()
        let session = LibraryBrowseSession(
            library: LibraryView.emptyLibrary,
            projectionRegistry: ProjectionRegistry(),
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        return LibraryView()
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(LibraryView.emptyLibrary)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .frame(width: 1100, height: 700)
            .windowBackground()
    }

    /// Enough synthesized albums for the grid to scroll well past the
    /// header's tracking zone, served through a canned-page `Library` behind
    /// the real session — scrolling the canvas drives the header collapse
    /// through the same `HeaderCollapse` pipeline the app uses. Expanding an
    /// album shows the detail placeholder (no release details are seeded).
    #Preview("Albums \u{2014} Grid") {
        let uiStore = UiStore()
        let libraryStore = LibraryStore()
        let albums: [BridgeAlbum] = (0..<40)
            .map { index in
                BridgeAlbum(
                    id: "grid-\(index)",
                    title: "Album Title \(index + 1)",
                    year: 1970 + Int32(index % 50),
                    isCompilation: false,
                    artistNames: "Artist Name \(index % 7 + 1)",
                    releaseIds: ["rel-grid-\(index)"],
                    primaryReleaseId: "rel-grid-\(index)",
                    cover: nil,
                )
            }
        let library = Library(
            getAlbumCount: { UInt64(albums.count) },
            getAlbumPage: { _, offset, limit in
                let start = min(Int(offset), albums.count)
                let end = min(start + Int(limit), albums.count)
                return Array(albums[start..<end])
            },
            getAlbumIndex: { _, albumId in
                albums.firstIndex { $0.id == albumId }.map(UInt64.init)
            },
        )
        let session = LibraryBrowseSession(
            library: library,
            projectionRegistry: ProjectionRegistry(),
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        return LibraryView()
            .environment(MediaPaths.stub)
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(library)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .frame(width: 1100, height: 700)
            .windowBackground()
    }

    #Preview("Artists \u{2014} Empty") {
        let uiStore = UiStore()
        uiStore.setLibraryBrowserMode(.artists)
        let libraryStore = LibraryStore()
        let session = LibraryBrowseSession(
            library: LibraryView.emptyLibrary,
            projectionRegistry: ProjectionRegistry(),
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        return LibraryView()
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(LibraryView.emptyLibrary)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .frame(width: 1100, height: 700)
            .windowBackground()
    }
#endif
