import SwiftUI

struct LibraryView: View {
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(Library.self)
    var library
    @Environment(LibraryStore.self)
    var libraryStore
    @Environment(UiStore.self)
    var uiStore

    @State
    private var list: AlbumList?
    @State
    private var sortCriteria: [BridgeSortCriterion] = Self.loadSortCriteria()

    var body: some View {
        @Bindable
        var uiStore = uiStore
        Group {
            if let list {
                if list.totalCount == 0 {
                    ContentUnavailableView(
                        "No albums",
                        systemImage: "square.stack",
                        description: Text("Import some music to get started"),
                    )
                }
                else {
                    AlbumGridView(
                        list: list,
                        selectedAlbumId: $uiStore.selectedAlbumId,
                        sortCriteria: $sortCriteria,
                        availableFields: BridgeSortField.allCases,
                        onPlay: { releaseId in
                            playback.playRelease(releaseId, nil, false)
                        },
                        onAddToQueue: { releaseId in
                            queue.addReleaseToQueue(releaseId)
                        },
                        onAddNext: { releaseId in
                            queue.addReleaseNext(releaseId)
                        },
                        headerTitle: String(localized: "Library"),
                    ) { albumId in
                        AlbumDetailView(albumId: albumId)
                    }
                }
            }
            else {
                ProgressView()
            }
        }
        .task {
            if list == nil {
                let newList = makeList(sort: sortCriteria)
                await newList.loadInitial()
                list = newList
            }
        }
        .onChange(of: sortCriteria) { _, newValue in
            Self.saveSortCriteria(newValue)
            let newList = makeList(sort: newValue)
            Task {
                await newList.loadInitial()
                list = newList
            }
        }
        .onReceive(libraryStore.libraryShapeSubject) { change in
            // Library grid rows are albums; release-level shape changes
            // don't move rows.
            switch change {
            case .albumAdded, .albumUpdated, .albumRemoved:
                list?.invalidate()
            case .releaseAdded, .releaseUpdated, .releaseRemoved:
                break
            }
        }
    }

    private func makeList(sort: [BridgeSortCriterion]) -> AlbumList {
        AlbumList(
            pageSource: LibraryAlbumPageSource(
                library: library,
                sort: sort
            ),
            ingest: { [libraryStore] rows in
                for row in rows {
                    _ = libraryStore.internAlbumSummary(row)
                }
            },
        )
    }

    private static let sortCriteriaKey = "librarySortCriteria"

    private static func loadSortCriteria() -> [BridgeSortCriterion] {
        guard let data = UserDefaults.standard.data(forKey: sortCriteriaKey),
            let criteria = [BridgeSortCriterion].fromJSON(data),
            !criteria.isEmpty
        else {
            return [
                BridgeSortCriterion(field: .dateAdded, direction: .descending)
            ]
        }
        return criteria
    }

    private static func saveSortCriteria(_ criteria: [BridgeSortCriterion]) {
        if let data = criteria.toJSON() {
            UserDefaults.standard.set(data, forKey: sortCriteriaKey)
        }
    }
}
