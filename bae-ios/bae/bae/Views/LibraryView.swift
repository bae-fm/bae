import Combine
import SwiftUI

private let pageSize = 60

private enum LibraryBrowserMode {
    case albums
    case composers
}

private enum LibraryRoute: Hashable {
    case album(
        albumId: String,
        initialReleaseId: String?,
        context: AlbumDetailContext?
    )
    case composer(String)
    case work(String)
}

/// Library browse root: a top bar with sync status, an album grid paged from
/// the database, navigation into album detail, and a persistent now-playing
/// bar. The grid re-queries whenever the library shape changes (sync streams
/// albums in over time), driven by `LibraryStore.libraryShapeSubject` →
/// `AlbumList.invalidate()`.
struct LibraryView: View {
    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(Library.self)
    private var library
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Sync.self)
    private var sync
    @Environment(Playback.self)
    private var playback

    @State
    private var showSettings = false
    @State
    private var mode: LibraryBrowserMode = .albums
    @State
    private var albumList: AlbumList?
    @State
    private var composerList: ComposerList?
    @State
    private var routePath: [LibraryRoute] = []
    @State
    private var searchQuery = ""
    @State
    private var searchResults: SearchResults?
    @State
    private var searchError: String?
    // Newest-first by default, matching the desktop library. Held as the two
    // enum components (each Equatable) so onChange can rebuild the paged list.
    @State
    private var sortField = BridgeSortField.dateAdded
    @State
    private var sortDirection = BridgeSortDirection.descending
    @State
    private var composerSortCriterion =
        BridgeComposerSortCriterion(field: .name, direction: .ascending)

    var body: some View {
        NavigationStack(path: $routePath) {
            VStack(spacing: 0) {
                LibraryBanner()
                LibraryModePicker(mode: $mode)
                content
            }
            .background(Theme.background)
            .navigationDestination(for: LibraryRoute.self) { route in
                switch route {
                case .album(let albumId, let releaseId, let context):
                    AlbumDetailView(
                        albumId: albumId,
                        initialReleaseId: releaseId,
                        context: context
                    )
                case .composer(let artistId):
                    ComposerDetailScreen(
                        artistId: artistId,
                        openWork: { routePath.append(.work($0)) },
                        openAlbum: { albumId, releaseId in
                            routePath.appendAlbum(
                                albumId: albumId,
                                initialReleaseId: releaseId,
                                context: nil
                            )
                        }
                    )
                case .work(let workId):
                    WorkDetailScreen(
                        workId: workId,
                        openWork: { routePath.append(.work($0)) },
                        openAlbum: { release in
                            routePath.appendAlbum(
                                albumId: release.albumId,
                                initialReleaseId: release.releaseId,
                                context: AlbumDetailContext(workRelease: release)
                            )
                        }
                    )
                }
            }
            .navigationTitle("bae")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(
                text: $searchQuery,
                prompt: "Search albums, tracks, composers, and works"
            )
            .task(id: searchQuery) {
                await runSearch()
            }
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    switch mode {
                    case .albums:
                        AlbumSortMenu(
                            sortField: $sortField,
                            sortDirection: $sortDirection
                        )
                    case .composers:
                        ComposerSortMenu(criterion: $composerSortCriterion)
                    }
                }
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        playback.playLibraryShuffled()
                    } label: {
                        Image(systemName: "shuffle")
                    }
                    .accessibilityLabel(Text("Shuffle Library"))
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Text(
                        configStore.syncReady
                            ? String(localized: "synced")
                            : String(localized: "syncing\u{2026}")
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        showSettings = true
                    } label: {
                        Image(systemName: "gearshape")
                    }
                    .accessibilityLabel("Settings")
                }
            }
            .sheet(isPresented: $showSettings) {
                SettingsView()
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                NowPlayingBar()
            }
        }
        .task(id: mode) {
            switch mode {
            case .albums:
                if albumList == nil {
                    await rebuildList()
                }
            case .composers:
                if composerList == nil {
                    await rebuildComposerList()
                }
            }
        }
        .onChange(of: sortField) { Task { await rebuildList() } }
        .onChange(of: sortDirection) { Task { await rebuildList() } }
        .onChange(of: composerSortCriterion) {
            Task { await rebuildComposerList() }
        }
        .onReceive(libraryStore.libraryShapeSubject) { change in
            switch change {
            case .albumAdded, .albumUpdated, .albumRemoved:
                albumList?.invalidate()
                composerList?.invalidate()
            case .releaseAdded, .releaseUpdated, .releaseRemoved:
                composerList?.invalidate()
            }
        }
    }

    private var isSearching: Bool {
        !searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// Rebuild the paged album list for the current sort — on first appear and
    /// whenever the sort field/direction changes.
    private func rebuildList() async {
        let albumList = AlbumList(
            pageSource: LibraryAlbumPageSource(
                library: library,
                sort: [
                    BridgeSortCriterion(field: sortField, direction: sortDirection)
                ]
            ),
            ingest: { rows in
                for row in rows {
                    _ = libraryStore.internAlbumSummary(row)
                }
            },
            onError: { error in
                configStore.showError(error)
            },
        )
        self.albumList = albumList
        await albumList.loadInitial()
    }

    private func rebuildComposerList() async {
        let list = ComposerList(
            pageSource: LibraryComposerPageSource(
                library: library,
                sort: composerSortCriterion
            ),
            ingest: { rows in
                for row in rows {
                    _ = libraryStore.internComposerSummary(row)
                }
            },
            onError: { error in
                configStore.showError(error)
            },
        )
        composerList = list
        await list.loadInitial()
    }

    @ViewBuilder
    private var content: some View {
        LibraryContentView(
            isSearching: isSearching,
            mode: mode,
            albumList: albumList,
            composerList: composerList,
            searchResults: searchResults,
            searchError: searchError,
            sync: sync,
            onSelectAlbum: {
                routePath.appendAlbum(
                    albumId: $0,
                    initialReleaseId: nil,
                    context: nil
                )
            },
            onSelectComposer: { routePath.append(.composer($0)) },
            onSelectWork: { routePath.append(.work($0)) }
        )
    }

    /// Debounced library search. `.task(id: searchQuery)` cancels the prior run
    /// on each keystroke: a pending 300ms debounce unwinds, and the post-await
    /// `Task.checkCancellation()` drops a superseded query's results before they
    /// are assigned — so only the latest query's results land. Prior results
    /// stay on screen while the next search runs.
    private func runSearch() async {
        let query = searchQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        searchError = nil
        guard !query.isEmpty else {
            searchResults = nil
            return
        }
        do {
            try await Task.sleep(for: .milliseconds(300))
            let bridge = try await library.searchLibrary(query)
            try Task.checkCancellation()
            searchResults = SearchResults(bridge: bridge, query: query)
        }
        catch is CancellationError {
            // Superseded by a newer query (or cleared); leave prior results.
        }
        catch {
            searchError = error.localizedDescription
        }
    }
}

private struct LibraryBanner: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Sync.self)
    private var sync

    var body: some View {
        if let error = configStore.lastError {
            banner(message: error.line) {
                Button {
                    configStore.clearError()
                } label: {
                    Image(systemName: "xmark")
                        .font(.caption.bold())
                        .foregroundStyle(Color.white)
                }
                .accessibilityLabel("Dismiss")
            }
        }
        else if let error = configStore.syncError {
            banner(message: error.line) {
                Button("Retry") { sync.triggerSync() }
                    .font(.caption.bold())
                    .foregroundStyle(Color.white)
            }
        }
    }

    private func banner<Trailing: View>(
        message: String,
        @ViewBuilder trailing: () -> Trailing
    ) -> some View {
        HStack {
            Text(message)
                .font(.caption)
                .foregroundStyle(Color.white)
                .frame(maxWidth: .infinity, alignment: .leading)
            trailing()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(Color.red.opacity(0.7))
    }
}

private struct LibraryModePicker: View {
    @Binding
    var mode: LibraryBrowserMode

    var body: some View {
        Picker("Library", selection: $mode) {
            Text("Albums").tag(LibraryBrowserMode.albums)
            Text("Composers").tag(LibraryBrowserMode.composers)
        }
        .pickerStyle(.segmented)
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }
}

private struct AlbumSortMenu: View {
    @Binding
    var sortField: BridgeSortField
    @Binding
    var sortDirection: BridgeSortDirection

    var body: some View {
        Menu {
            ForEach(BridgeSortField.allCases, id: \.self) { field in
                Button {
                    sortField = field
                } label: {
                    if field == sortField {
                        Label(field.displayName, systemImage: "checkmark")
                    }
                    else {
                        Text(field.displayName)
                    }
                }
            }
            Divider()
            Button {
                sortDirection =
                    sortDirection == .ascending ? .descending : .ascending
            } label: {
                Label(
                    sortDirection == .ascending
                        ? String(localized: "Ascending")
                        : String(localized: "Descending"),
                    systemImage: sortDirection == .ascending
                        ? "arrow.up" : "arrow.down"
                )
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
        }
    }
}

private struct ComposerSortMenu: View {
    @Binding
    var criterion: BridgeComposerSortCriterion

    var body: some View {
        Menu {
            ForEach(BridgeComposerSortField.allCases, id: \.self) { field in
                Button {
                    criterion = BridgeComposerSortCriterion(
                        field: field,
                        direction: criterion.direction
                    )
                } label: {
                    if field == criterion.field {
                        Label(field.displayName, systemImage: "checkmark")
                    }
                    else {
                        Text(field.displayName)
                    }
                }
            }
            Divider()
            Button {
                let direction: BridgeSortDirection =
                    criterion.direction == .ascending ? .descending : .ascending
                criterion = BridgeComposerSortCriterion(
                    field: criterion.field,
                    direction: direction
                )
            } label: {
                Label(
                    criterion.direction == .ascending
                        ? String(localized: "Ascending")
                        : String(localized: "Descending"),
                    systemImage: criterion.direction == .ascending
                        ? "arrow.up" : "arrow.down"
                )
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
        }
    }
}

private struct LibraryContentView: View {
    let isSearching: Bool
    let mode: LibraryBrowserMode
    let albumList: AlbumList?
    let composerList: ComposerList?
    let searchResults: SearchResults?
    let searchError: String?
    let sync: Sync
    let onSelectAlbum: (String) -> Void
    let onSelectComposer: (String) -> Void
    let onSelectWork: (String) -> Void

    var body: some View {
        if isSearching {
            SearchResultsView(
                results: searchResults,
                error: searchError,
                onSelectAlbum: onSelectAlbum,
                onSelectComposer: onSelectComposer,
                onSelectWork: onSelectWork
            )
        }
        else {
            switch mode {
            case .albums:
                albumContent
            case .composers:
                composerContent
            }
        }
    }

    @ViewBuilder
    private var albumContent: some View {
        if let albumList {
            AlbumGrid(list: albumList, onSelect: onSelectAlbum)
                .refreshable {
                    sync.triggerSync()
                    await settleAfterRefresh()
                }
        }
        else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    @ViewBuilder
    private var composerContent: some View {
        if let composerList {
            ComposerListView(list: composerList, onSelect: onSelectComposer)
                .refreshable {
                    sync.triggerSync()
                    await settleAfterRefresh()
                }
        }
        else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func settleAfterRefresh() async {
        do {
            try await Task.sleep(for: .seconds(0.9))
        }
        catch is CancellationError {}
        catch {
            preconditionFailure("Unexpected refresh sleep error: \(error)")
        }
    }
}

private extension Array where Element == LibraryRoute {
    mutating func appendAlbum(
        albumId: String,
        initialReleaseId: String?,
        context: AlbumDetailContext?
    ) {
        append(
            .album(
                albumId: albumId,
                initialReleaseId: initialReleaseId,
                context: context
            )
        )
    }
}

/// The paged album grid. Renders one card per loaded slot, resolving each
/// album's summary from the store and its cover from the primary release's
/// on-disk file. Rows that aren't loaded yet kick off their page load via
/// `.task(id:)` keyed on the list's `loadEpoch`.
private struct AlbumGrid: View {
    let list: AlbumList
    let onSelect: (String) -> Void

    private let columns = [GridItem(.adaptive(minimum: 150), spacing: 12)]

    var body: some View {
        if list.totalCount == 0 {
            Text("No albums yet. Syncing from the cloud\u{2026}")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(32)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        else {
            ScrollView {
                LazyVGrid(columns: columns, spacing: 12) {
                    ForEach(0..<list.totalCount, id: \.self) { position in
                        AlbumCell(
                            list: list,
                            position: position,
                            onSelect: onSelect
                        )
                    }
                }
                .padding(12)
            }
        }
    }
}

/// One grid slot. Loads the page covering its position (keyed on `loadEpoch` so
/// a list swap or invalidation re-fetches), then renders the album once its id
/// resolves.
private struct AlbumCell: View {
    let list: AlbumList
    let position: Int
    let onSelect: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        Group {
            if let albumId = list.idAt(position),
                let summary = libraryStore.albumSummaries[albumId]
            {
                AlbumCard(
                    summary: summary,
                    onTap: { onSelect(albumId) }
                )
            }
            else {
                Theme.placeholder
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
        }
        .task(id: list.loadEpoch) {
            let offset = (position / pageSize) * pageSize
            await list.loadRange(offset: offset, limit: pageSize)
        }
    }
}

private struct AlbumCard: View {
    let summary: AlbumSummary
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 6) {
                ImageView(imageRef: summary.cover, pointSize: 150)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                Text(summary.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                Text(summary.artistNames)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .buttonStyle(.plain)
        // One VoiceOver element per card: announce "Title by Artist" instead of
        // the cover image + two separate text fragments. The cover is decorative
        // (its info is in the text), so it's folded into the combined label.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            summary.artistNames.isEmpty
                ? summary.title
                : String(
                    localized: "\(summary.title) by \(summary.artistNames)",
                    comment: "Album card VoiceOver label: title by artist"
                )
        )
    }
}

private struct ComposerListView: View {
    let list: ComposerList
    let onSelect: (String) -> Void

    var body: some View {
        if list.totalCount == 0 {
            Text("No composers")
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding(32)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        else {
            List {
                ForEach(0..<list.totalCount, id: \.self) { position in
                    ComposerRowSlot(
                        list: list,
                        position: position,
                        onSelect: onSelect
                    )
                }
            }
            .listStyle(.plain)
        }
    }
}

private struct ComposerRowSlot: View {
    let list: ComposerList
    let position: Int
    let onSelect: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        Group {
            if let id = list.idAt(position),
                let summary = libraryStore.composerSummaries[id]
            {
                Button {
                    onSelect(id)
                } label: {
                    ComposerSummaryRow(summary: summary)
                }
                .buttonStyle(.plain)
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task(id: list.loadEpoch) {
            let offset = (position / pageSize) * pageSize
            await list.loadRange(offset: offset, limit: pageSize)
        }
    }
}

private struct ComposerSummaryRow: View {
    let summary: BridgeComposerSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary.image, pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 2) {
                Text(summary.name)
                    .font(.body)
                    .lineLimit(1)
                Text("\(summary.workCount) \(String(localized: "Works"))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
        }
        .padding(.vertical, 4)
    }
}

private struct ComposerDetailScreen: View {
    let artistId: String
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    @Environment(Library.self)
    private var library

    @State
    private var detail: BridgeComposerDetail?
    @State
    private var error: String?

    var body: some View {
        Group {
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .padding(32)
            }
            else if let detail {
                ComposerDetailContent(
                    detail: detail,
                    openWork: openWork,
                    openAlbum: openAlbum
                )
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: artistId) {
            await load()
        }
    }

    private var navigationTitle: String {
        if let detail {
            return detail.composer.name
        }
        if error != nil {
            return String(localized: "Composers")
        }
        return ""
    }

    private func load() async {
        error = nil
        do {
            let getComposerDetail = library.getComposerDetail
            let loaded =
                try await Task.detached {
                    try getComposerDetail(artistId)
                }
                .value
            try Task.checkCancellation()
            guard let loaded else {
                error = String(localized: "Composer detail not found")
                return
            }
            detail = loaded
        }
        catch is CancellationError {}
        catch {
            self.error = error.localizedDescription
        }
    }
}

private struct ComposerDetailContent: View {
    let detail: BridgeComposerDetail
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    var body: some View {
        List {
            Section {
                ComposerSummaryRow(summary: detail.composer)
            }
            if !detail.workGroups.isEmpty {
                Section("Works") {
                    ForEach(detail.workGroups, id: \.id) { group in
                        if let parent = group.parent {
                            WorkSummaryButton(summary: parent, openWork: openWork)
                        }
                        ForEach(group.works, id: \.workId) { work in
                            WorkSummaryButton(summary: work, openWork: openWork)
                        }
                    }
                }
            }
            if !detail.unlinkedReleaseRoles.isEmpty {
                Section("Credits") {
                    ForEach(detail.unlinkedReleaseRoles, id: \.releaseId) {
                        role in
                        Button {
                            openAlbum(role.albumId, role.releaseId)
                        } label: {
                            TwoLineRow(
                                title: role.albumTitle,
                                subtitle: role.sourceCredit
                            )
                        }
                    }
                }
            }
            if !detail.unlinkedTrackRoles.isEmpty {
                Section("Recordings") {
                    ForEach(detail.unlinkedTrackRoles, id: \.trackId) { role in
                        TwoLineRow(
                            title: role.trackTitle,
                            subtitle: role.albumTitle
                        )
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
    }
}

private struct WorkDetailScreen: View {
    let workId: String
    let openWork: (String) -> Void
    let openAlbum: (BridgeWorkReleaseSummary) -> Void

    @Environment(Library.self)
    private var library

    @State
    private var detail: BridgeWorkDetail?
    @State
    private var error: String?

    var body: some View {
        Group {
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .padding(32)
            }
            else if let detail {
                WorkDetailContent(
                    detail: detail,
                    openWork: openWork,
                    openAlbum: openAlbum
                )
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: workId) {
            await load()
        }
    }

    private var navigationTitle: String {
        if let detail {
            return detail.work.title
        }
        if error != nil {
            return String(localized: "Works")
        }
        return ""
    }

    private func load() async {
        error = nil
        do {
            let getWorkDetail = library.getWorkDetail
            let loaded =
                try await Task.detached {
                    try getWorkDetail(workId)
                }
                .value
            try Task.checkCancellation()
            guard let loaded else {
                error = String(localized: "Work detail not found")
                return
            }
            detail = loaded
        }
        catch is CancellationError {}
        catch {
            self.error = error.localizedDescription
        }
    }
}

private struct WorkDetailContent: View {
    let detail: BridgeWorkDetail
    let openWork: (String) -> Void
    let openAlbum: (BridgeWorkReleaseSummary) -> Void

    var body: some View {
        List {
            Section {
                WorkSummaryRow(summary: detail.work)
            }
            if !detail.childWorks.isEmpty {
                Section("Works") {
                    ForEach(detail.childWorks, id: \.workId) { work in
                        WorkSummaryButton(summary: work, openWork: openWork)
                    }
                }
            }
            if !detail.releases.isEmpty {
                Section("Releases") {
                    ForEach(detail.releases, id: \.releaseId) { release in
                        Button {
                            openAlbum(release)
                        } label: {
                            HStack(spacing: 12) {
                                ImageView(imageRef: release.cover, pointSize: 42)
                                    .frame(width: 42, height: 42)
                                    .clipShape(RoundedRectangle(cornerRadius: 6))
                                TwoLineRow(
                                    title: release.albumTitle,
                                    subtitle: workReleaseMetadata(release)
                                )
                            }
                        }
                    }
                }
            }
            if !detail.tracks.isEmpty {
                Section("Recordings") {
                    ForEach(detail.tracks, id: \.trackId) { track in
                        TwoLineRow(
                            title: track.trackTitle,
                            subtitle: track.albumTitle
                        )
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
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

private struct WorkSummaryButton: View {
    let summary: BridgeWorkSummary
    let openWork: (String) -> Void

    var body: some View {
        Button {
            openWork(summary.workId)
        } label: {
            WorkSummaryRow(summary: summary)
        }
    }
}

private struct WorkSummaryRow: View {
    let summary: BridgeWorkSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary.representativeCover, pointSize: 42)
                .frame(width: 42, height: 42)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            TwoLineRow(title: summary.title, subtitle: summary.composerNames)
        }
        .padding(.vertical, 4)
    }
}

private struct TwoLineRow: View {
    let title: String
    let subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.body)
                .lineLimit(1)
            if let subtitle, !subtitle.isEmpty {
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
    }
}
