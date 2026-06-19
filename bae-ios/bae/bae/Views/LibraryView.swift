import Combine
import SwiftUI

private let pageSize = 60

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
    @Environment(MediaPaths.self)
    private var mediaPaths
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Sync.self)
    private var sync

    @State
    private var showSettings = false
    @State
    private var list: AlbumList?
    @State
    private var selectedAlbumId: String?
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

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                banner
                content
            }
            .background(Theme.background)
            .navigationDestination(item: $selectedAlbumId) { albumId in
                AlbumDetailView(albumId: albumId)
            }
            .navigationTitle("bae")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(text: $searchQuery, prompt: "Search albums and tracks")
            .task(id: searchQuery) {
                await runSearch()
            }
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    sortMenu
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
        .task {
            if list == nil {
                await rebuildList()
            }
        }
        .onChange(of: sortField) { Task { await rebuildList() } }
        .onChange(of: sortDirection) { Task { await rebuildList() } }
        .onReceive(libraryStore.libraryShapeSubject) { change in
            // Grid rows are albums, so only album-level shape changes move rows.
            switch change {
            case .albumAdded, .albumUpdated, .albumRemoved:
                list?.invalidate()
            case .releaseAdded, .releaseUpdated, .releaseRemoved:
                break
            }
        }
    }

    @ViewBuilder
    private var banner: some View {
        // An app error is a one-shot notification with nothing to retry, so the
        // user dismisses it. A sync error is live state — it clears itself when
        // sync recovers — so it offers Retry instead of a (misleading) dismiss.
        // Both arrive as a typed `DisplayError`; the banner shows its localized
        // line (the opaque diagnostic detail isn't surfaced in the mobile UI).
        if let error = configStore.lastError {
            errorBanner(message: error.line) {
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
            errorBanner(message: error.line) {
                Button("Retry") { sync.triggerSync() }
                    .font(.caption.bold())
                    .foregroundStyle(Color.white)
            }
        }
    }

    private func errorBanner<Trailing: View>(
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
            }
        )
        list = albumList
        await albumList.loadInitial()
    }

    private var sortMenu: some View {
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

    @ViewBuilder
    private var content: some View {
        if isSearching {
            SearchResultsView(
                results: searchResults,
                error: searchError,
                onSelect: { selectedAlbumId = $0 }
            )
        }
        else if let list {
            AlbumGrid(
                list: list,
                onSelect: { selectedAlbumId = $0 }
            )
            .refreshable {
                // Re-kick sync; results stream back via library events. Hold the
                // spinner briefly to acknowledge the pull (sync has no
                // in-progress signal to await).
                sync.triggerSync()
                try? await Task.sleep(for: .seconds(0.9))
            }
        }
        else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
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

/// The paged album grid. Renders one card per loaded slot, resolving each
/// album's summary from the store and its cover from the primary release's
/// on-disk file. Rows that aren't loaded yet kick off their page load via
/// `.task(id:)` keyed on the list's `loadEpoch`.
private struct AlbumGrid: View {
    let list: AlbumList
    let onSelect: (String) -> Void

    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(MediaPaths.self)
    private var mediaPaths

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
    @Environment(MediaPaths.self)
    private var mediaPaths

    var body: some View {
        Group {
            if let albumId = list.idAt(position),
                let summary = libraryStore.albumSummaries[albumId]
            {
                AlbumCard(
                    summary: summary,
                    coverPath: mediaPaths.imagePathIfExists(
                        summary.primaryReleaseId
                    ),
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
    let coverPath: String?
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 6) {
                ImageView(path: coverPath, pointSize: 150)
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
