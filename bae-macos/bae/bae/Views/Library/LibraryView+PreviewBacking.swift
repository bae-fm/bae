#if DEBUG
    import BaeKit
    import SwiftUI

    private final class PreviewLibrarySubscription: LiveSubscriptionProtocol,
        @unchecked Sendable
    {
        func cancel() {}
    }

    // Canned `Library` + `LibraryBrowseSession` backings for the `LibraryView`
    // previews (and the whole-window `MainAppView` preview, which reuses
    // `previewGridBacking`). Each drives the production body through a real
    // session over a canned-page library, so previews exercise the actual
    // empty-state, grid, and composer-detail branches rather than stand-ins.
    extension LibraryView {
        /// A `Library` whose album/composer counts and pages are all empty,
        /// driving the real `loadInitial()` → `totalCount == 0` path so the
        /// previews below hit the actual empty-state branches in
        /// `albumContent`/`composerContent` rather than a hand-built stand-in.
        static func emptyLibrary() -> Library {
            Library(
                subscribeAlbumPage: { _, _, _, callback in
                    callback.onValue(
                        value: BridgeAlbumPage(rows: [], totalCount: 0)
                    )
                    return PreviewLibrarySubscription()
                },
                subscribeComposerPage: { _, _, _, callback in
                    callback.onValue(
                        value: BridgeComposerPage(rows: [], totalCount: 0)
                    )
                    return PreviewLibrarySubscription()
                },
                subscribeArtistPage: { _, _, _, callback in
                    callback.onValue(
                        value: BridgeArtistPage(rows: [], totalCount: 0)
                    )
                    return PreviewLibrarySubscription()
                }
            )
        }

        /// `emptyLibrary` behind a real session — the backing for the empty
        /// previews and the whole-window empty scene (desktop story 3), so
        /// they exercise the actual zero-count empty-state branches.
        @MainActor
        static func previewEmptyBacking(
            uiStore: UiStore,
            libraryStore: LibraryStore
        ) -> (library: Library, session: LibraryBrowseSession) {
            let library = emptyLibrary()
            let session = LibraryBrowseSession(
                library: library,
                libraryStore: libraryStore,
                uiStore: uiStore
            )
            return (library, session)
        }

        /// Enough synthesized albums for the grid to scroll well past the
        /// header's tracking zone, served through a canned-page `Library`
        /// behind a real session — the populated backing for whole-screen
        /// previews. Scrolling drives the header collapse through the same
        /// `HeaderCollapse` pipeline the app uses. Expanding an album shows
        /// the detail placeholder (no release details are seeded).
        @MainActor
        static func previewGridBacking(
            uiStore: UiStore,
            libraryStore: LibraryStore
        ) -> (library: Library, session: LibraryBrowseSession) {
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
                subscribeAlbumPage: { _, offset, limit, callback in
                    let start = min(Int(offset), albums.count)
                    let end = min(start + Int(limit), albums.count)
                    callback.onValue(
                        value: BridgeAlbumPage(
                            rows: Array(albums[start..<end]),
                            totalCount: UInt64(albums.count)
                        )
                    )
                    return PreviewLibrarySubscription()
                },
                getAlbumIndex: { _, albumId in
                    albums.firstIndex { $0.id == albumId }.map(UInt64.init)
                },
            )
            let session = LibraryBrowseSession(
                library: library,
                libraryStore: libraryStore,
                uiStore: uiStore
            )
            return (library, session)
        }

        /// A canned composer library — a master list plus one composer's detail
        /// (works, releases, recordings) — behind a real session, so the
        /// composer detail preview renders the restyled master list and detail
        /// pane through the production `LibraryView` body. Images are absent, so
        /// every slot shows the placeholder treatment.
        @MainActor
        static func previewComposerBacking(
            uiStore: UiStore,
            libraryStore: LibraryStore
        ) -> (library: Library, session: LibraryBrowseSession) {
            let composers = previewComposers()
            let works: [BridgeWorkSummary] = (0..<4)
                .map { (index: Int) -> BridgeWorkSummary in
                    BridgeWorkSummary(
                        workId: "work-\(index)",
                        title: "Work Title \(index + 1)",
                        disambiguation: nil,
                        workType: nil,
                        parentWorkId: nil,
                        composerNames: "Composer Name 1",
                        linkedReleaseCount: Int64(1 + index),
                        representativeReleaseId: nil,
                        representativeCover: nil,
                    )
                }
            let composerDetail = previewComposerDetail(
                composer: composers[0],
                works: works
            )
            let workDetail = previewWorkDetail(work: works[0])
            let library = Library(
                subscribeComposerPage: { _, offset, limit, callback in
                    let start = min(Int(offset), composers.count)
                    let end = min(start + Int(limit), composers.count)
                    callback.onValue(
                        value: BridgeComposerPage(
                            rows: Array(composers[start..<end]),
                            totalCount: UInt64(composers.count)
                        )
                    )
                    return PreviewLibrarySubscription()
                },
                subscribeComposerDetail: { _, callback in
                    callback.onValue(value: composerDetail)
                    return PreviewLibrarySubscription()
                },
                subscribeWorkDetail: { _, callback in
                    callback.onValue(value: workDetail)
                    return PreviewLibrarySubscription()
                },
            )
            let session = LibraryBrowseSession(
                library: library,
                libraryStore: libraryStore,
                uiStore: uiStore
            )
            session.selectComposer("composer-0")
            return (library, session)
        }

        private static func previewComposers() -> [BridgeComposerSummary] {
            (0..<14)
                .map { (index: Int) -> BridgeComposerSummary in
                    BridgeComposerSummary(
                        artistId: "composer-\(index)",
                        name: "Composer Name \(index + 1)",
                        sortName: nil,
                        workCount: Int64(2 + index % 6),
                        linkedReleaseCount: Int64(3 + index % 4),
                        unlinkedCreditCount: 0,
                        image: nil,
                    )
                }
        }

        private static func previewComposerDetail(
            composer: BridgeComposerSummary,
            works: [BridgeWorkSummary]
        ) -> BridgeComposerDetail {
            BridgeComposerDetail(
                composer: composer,
                workGroups: [
                    BridgeComposerWorkGroup(
                        id: "group-0",
                        parent: nil,
                        works: works
                    )
                ],
                unlinkedReleaseRoles: [],
                unlinkedTrackRoles: [],
                defaultWorkId: "work-0",
            )
        }

        private static func previewWorkDetail(
            work: BridgeWorkSummary
        ) -> BridgeWorkDetail {
            BridgeWorkDetail(
                work: work,
                childWorks: [],
                releases: (0..<3)
                    .map { (index: Int) -> BridgeWorkReleaseSummary in
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
                    .map { (index: Int) -> BridgeWorkTrackSummary in
                        BridgeWorkTrackSummary(
                            trackId: "track-\(index)",
                            trackTitle: "Track Title \(index + 1)",
                            releaseId: "release-0",
                            albumId: "album-0",
                            albumTitle: "Album Title 1",
                        )
                    },
            )
        }
    }
#endif
