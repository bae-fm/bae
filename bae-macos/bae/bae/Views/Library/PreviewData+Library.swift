#if DEBUG
    import BaeKit
    import SwiftUI

    // Preview fixtures for the library browse views: composer/artist/work
    // summaries and details, plus seeded `LibraryStore`s and a
    // `LibraryBrowseSession` builder. Generic placeholder names only. Reuses
    // the base `PreviewData.albums` for anything that needs `BridgeAlbum`s.
    extension PreviewData {
        // MARK: - Composers

        static let composerSummary = BridgeComposerSummary(
            artistId: "composer-0",
            name: "Composer Name",
            sortName: nil,
            workCount: 6,
            linkedReleaseCount: 4,
            unlinkedCreditCount: 2,
            image: nil,
        )

        /// A master list of composers for the browse-list previews.
        static let composerSummaries: [BridgeComposerSummary] = (0..<12)
            .map { (index: Int) -> BridgeComposerSummary in
                let workCount = Int64(2 + index % 6)
                let linkedReleaseCount = Int64(1 + index % 4)
                return BridgeComposerSummary(
                    artistId: "composer-\(index)",
                    name: "Composer Name \(index + 1)",
                    sortName: nil,
                    workCount: workCount,
                    linkedReleaseCount: linkedReleaseCount,
                    unlinkedCreditCount: 0,
                    image: nil,
                )
            }

        static let composerWorkGroup = BridgeComposerWorkGroup(
            id: "group-0",
            parent: workSummaries[0],
            works: Array(workSummaries[1...]),
        )

        static let composerDetail = BridgeComposerDetail(
            composer: composerSummary,
            workGroups: [
                BridgeComposerWorkGroup(
                    id: "group-0",
                    parent: nil,
                    works: workSummaries,
                )
            ],
            unlinkedReleaseRoles: [
                BridgeReleaseRoleSummary(
                    releaseId: "release-credit-0",
                    albumId: "album-credit-0",
                    albumTitle: "Album Title",
                    source: .musicBrainz,
                    sourceCredit: "Arranger",
                )
            ],
            unlinkedTrackRoles: [
                BridgeTrackRoleSummary(
                    trackId: "track-credit-0",
                    trackTitle: "Track Title",
                    releaseId: "release-credit-0",
                    albumId: "album-credit-0",
                    albumTitle: "Album Title",
                    artistId: "artist-0",
                    artistName: "Artist Name",
                    source: .musicBrainz,
                    sourceCredit: "Orchestrator",
                )
            ],
            defaultWorkId: "work-0",
        )

        // MARK: - Works

        static let workSummaries: [BridgeWorkSummary] = (0..<4)
            .map { index in
                BridgeWorkSummary(
                    workId: "work-\(index)",
                    title: "Work Title \(index + 1)",
                    disambiguation: nil,
                    workType: nil,
                    parentWorkId: nil,
                    composerNames: "Composer Name",
                    linkedReleaseCount: Int64(1 + index),
                    representativeReleaseId: nil,
                    representativeCover: nil,
                )
            }

        static let workDetail = BridgeWorkDetail(
            work: workSummaries[0],
            childWorks: Array(workSummaries[1...2]),
            releases: (0..<3)
                .map { index in
                    BridgeWorkReleaseSummary(
                        releaseId: "release-\(index)",
                        albumId: "album-\(index)",
                        albumTitle: "Album Title \(index + 1)",
                        displayName: "Album Title \(index + 1)",
                        format: "2\u{00D7}LP",
                        cover: nil,
                    )
                },
            tracks: (0..<4)
                .map { index in
                    BridgeWorkTrackSummary(
                        trackId: "track-\(index)",
                        trackTitle: "Track Title \(index + 1)",
                        releaseId: "release-0",
                        albumId: "album-0",
                        albumTitle: "Album Title 1",
                    )
                },
        )

        // MARK: - Artists

        static let artistSummary = BridgeArtistSummary(
            artistId: "artist-0",
            name: "Artist Name",
            albumCount: 6,
            image: nil,
        )

        static let artistSummaries: [BridgeArtistSummary] = (0..<12)
            .map { index in
                BridgeArtistSummary(
                    artistId: "artist-\(index)",
                    name: "Artist Name \(index + 1)",
                    albumCount: Int64(1 + index % 8),
                    image: nil,
                )
            }

        static let artistDetail = BridgeArtistDetail(
            artist: artistSummary,
            albums: Array(albums.prefix(6)),
        )

        // MARK: - Grid card menu

        /// The bulk-action menu a grid card presents, with no-op actions.
        static func albumCardMenu(targetCount: Int = 1) -> AlbumCardMenu {
            AlbumCardMenu(
                targetCount: targetCount,
                onPlay: {},
                onAddToQueue: {},
                onAddNext: {},
                onPin: {},
            )
        }

        // MARK: - Seeded stores + session

        @MainActor
        static func seededComposerStore() -> LibraryStore {
            let store = LibraryStore()
            for summary in composerSummaries {
                store.internComposerSummary(summary)
            }
            return store
        }

        @MainActor
        static func seededArtistStore() -> LibraryStore {
            let store = LibraryStore()
            for summary in artistSummaries {
                store.internArtistSummary(summary)
            }
            return store
        }

        /// A `LibraryBrowseSession` over `Library.stub()` — the detail-pane
        /// previews drive their content through `paneDetail`/`detail` props,
        /// so the session only supplies the selection state the panes read.
        @MainActor
        static func browseSession(
            libraryStore: LibraryStore,
            uiStore: UiStore,
        ) -> LibraryBrowseSession {
            LibraryBrowseSession(
                library: .stub(),
                libraryStore: libraryStore,
                uiStore: uiStore,
            )
        }

        /// The composer-list slot backing for `BrowseList`, seeded so its rows
        /// render without an async page fetch (the seeded segment fast-paths
        /// `loadRange`).
        @MainActor
        static func composerList() -> ComposerList {
            let list = ComposerList(
                pageSource: LibraryComposerPageSource(
                    library: .stub(),
                    sort: []
                ),
                ingest: { _ in },
                onError: { _ in },
            )
            list.preloadForPreview(ids: composerSummaries.map(\.artistId))
            return list
        }
    }
#endif
