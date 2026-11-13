import BaeKit
import SwiftUI

@testable import bae

// Fixtures for the library browse views: composer/artist/work
// summaries and details, a seeded composer list, and a
// `LibraryBrowseSession` builder. Generic placeholder names only. Reuses
// the base `PreviewData.albums` for anything that needs `BridgeAlbum`s.
extension PreviewData {
    // MARK: - Composers

    static let composerSummary = BridgeComposerSummary(
        artistId: "composer-0",
        name: "Composer Name",
        sortName: nil,
        countLine: BridgeCountLabel(
            key: "core.library.work_count",
            count: 6
        ),
        linkedReleaseCount: 4,
        unlinkedCreditCount: 2,
        image: nil,
    )

    /// A master list of composers for the browse-list shots.
    static let composerSummaries: [BridgeComposerSummary] = (0..<12)
        .map { (index: Int) -> BridgeComposerSummary in
            let linkedReleaseCount = Int64(1 + index % 4)
            return BridgeComposerSummary(
                artistId: "composer-\(index)",
                name: "Composer Name \(index + 1)",
                sortName: nil,
                countLine: BridgeCountLabel(
                    key: "core.library.work_count",
                    count: UInt32(2 + index % 6)
                ),
                linkedReleaseCount: linkedReleaseCount,
                unlinkedCreditCount: 0,
                image: nil,
            )
        }

    static let composerDetail = BridgeComposerDetail(
        composer: composerSummary,
        defaultWorkId: "work-0",
    )

    /// One parent work with its parts (the indented branch of the works
    /// list), a lone work heading a group of its own, then a release
    /// credit and a track credit, each part of the page under its
    /// heading.
    static let composerDetailRowValues: [BridgeComposerDetailRow] = [
        BridgeComposerDetailRow(
            id: 0,
            heading: .works,
            item: .work(work: workSummaries[0], part: false)
        ),
        BridgeComposerDetailRow(
            id: 1,
            heading: nil,
            item: .work(work: workSummaries[1], part: true)
        ),
        BridgeComposerDetailRow(
            id: 2,
            heading: nil,
            item: .work(work: workSummaries[2], part: true)
        ),
        BridgeComposerDetailRow(
            id: 3,
            heading: nil,
            item: .work(work: workSummaries[3], part: false)
        ),
        BridgeComposerDetailRow(
            id: 4,
            heading: .credits,
            item: .releaseCredit(
                credit: BridgeReleaseRoleSummary(
                    releaseId: "release-credit-0",
                    albumId: "album-credit-0",
                    albumTitle: "Album Title",
                    role: .word("Arranger")
                )
            )
        ),
        BridgeComposerDetailRow(
            id: 5,
            heading: .recordings,
            item: .trackCredit(
                credit: BridgeTrackRoleSummary(
                    trackId: "track-credit-0",
                    trackTitle: "Track Title",
                    releaseId: "release-credit-0",
                    albumId: "album-credit-0",
                    albumTitle: "Album Title",
                    artistId: "artist-0",
                    artistName: "Artist Name",
                    role: .word("Orchestrator")
                )
            )
        ),
    ]

    /// The page rows of `rows`, seeded so they render without an async
    /// page fetch.
    @MainActor
    static func composerDetailRows(
        _ rows: [BridgeComposerDetailRow] = composerDetailRowValues
    ) -> ComposerDetailRowList {
        let list = ComposerDetailRowList(
            pageSource: LibraryComposerDetailPageSource(
                query: .fixed(rows, answers: UnansweredCount())
            ),
            onError: { _ in },
        )
        list.preloadRows(rows: rows)
        return list
    }

    /// A composer page whose works run far past any viewport: one lone
    /// work per row, each carrying its own cover reference, so a mounted
    /// row shows up as a distinct image request. Read through its fixed
    /// query a page at a time, as the app reads core's.
    @MainActor
    static func manyComposerDetailRows(
        workCount: Int
    ) -> ComposerDetailRowList {
        let rows = (0..<workCount)
            .map { (index: Int) -> BridgeComposerDetailRow in
                BridgeComposerDetailRow(
                    id: Int64(index),
                    heading: index == 0 ? .works : nil,
                    item: .work(
                        work: BridgeWorkSummary(
                            workId: "work-\(index)",
                            title: "Work Title \(index + 1)",
                            disambiguation: nil,
                            workType: nil,
                            composerNames: "Composer Name",
                            linkedReleaseCount: 1,
                            representativeReleaseId: nil,
                            representativeCover: BridgeImageRef(
                                id: "cover-\(index)",
                                version: "1",
                                imageType: .cover
                            ),
                        ),
                        part: false
                    )
                )
            }
        return ComposerDetailRowList(
            pageSource: LibraryComposerDetailPageSource(
                query: .fixed(rows, answers: UnansweredCount())
            ),
            onError: { _ in },
        )
    }

    // MARK: - Works

    static let workSummaries: [BridgeWorkSummary] = (0..<4)
        .map { index in
            BridgeWorkSummary(
                workId: "work-\(index)",
                title: "Work Title \(index + 1)",
                disambiguation: nil,
                workType: nil,
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
                    name: .named(name: "Album Title \(index + 1)"),
                    media: bridgeMediaTerms(
                        media: PreviewData.media(.vinyl, 2)
                    ),
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
        countLine: BridgeCountLabel(
            key: "core.library.album_count",
            count: 6
        ),
        image: nil,
    )

    static let artistDetail = BridgeArtistDetail(
        artist: artistSummary,
        creditNames: [
            BridgeArtistCreditName(
                artistId: artistSummary.artistId,
                credit: BridgeCreditName(
                    key: "artist alias",
                    name: "Artist Alias"
                )
            )
        ],
        albums: Array(albums.prefix(6)),
    )

    // MARK: - Session

    /// A `LibraryBrowseSession` over `Library.stub()` — the detail-pane
    /// shots drive their content through `paneDetail`/`detail` props,
    /// so the session only supplies the selection state the panes read.
    @MainActor
    static func browseSession(uiStore: UiStore) -> LibraryBrowseSession {
        LibraryBrowseSession(library: .stub(), uiStore: uiStore)
    }

    /// The composer-list slot backing for `BrowseList`, seeded so its rows
    /// render without an async page fetch (a page whose positions are
    /// seeded is never asked for).
    @MainActor
    static func composerList() -> ComposerList {
        let list = ComposerList(
            pageSource: LibraryComposerPageSource(
                query: .fixed(composerSummaries, answers: UnansweredCount())
            ),
            onError: { _ in },
        )
        list.preloadRows(rows: composerSummaries)
        return list
    }
}
