import BaeKit
import SwiftUI

@testable import bae

// This file gives the composer detail's test a canned `Library` and a
// `LibraryBrowseSession` that reads it, so the test drives the production
// `LibraryView` body. The file also holds the playlists read of a library
// with no playlists, which the shots' backings share.
extension LibraryView {
    /// The playlists read of a library with none: the library window reads
    /// them whenever it opens, as the app's does.
    static let noPlaylists:
        @Sendable () -> PulledValues<BridgePlaylistListing> = {
            .fixed(
                BridgePlaylistListing(
                    listed: [],
                    addTargets: [],
                    total: 0,
                    addTargetCount: 0,
                    canCreate: true
                )
            )
        }

    /// A canned composer library, with a master list and one composer's
    /// detail (works, releases, recordings), and a `LibraryBrowseSession`
    /// that reads it, so the composer detail's test renders the master list
    /// and detail pane through the production `LibraryView` body. Images are absent, so
    /// every slot shows the placeholder treatment.
    @MainActor
    static func previewComposerBacking(
        uiStore: UiStore
    ) -> (library: Library, session: LibraryBrowseSession) {
        let composers = previewComposers()
        let works: [BridgeWorkSummary] = (0..<4)
            .map { (index: Int) -> BridgeWorkSummary in
                BridgeWorkSummary(
                    workId: "work-\(index)",
                    title: "Work Title \(index + 1)",
                    composerNames: "Composer Name 1",
                    linkedReleaseCount: Int64(1 + index),
                    representativeCover: nil,
                )
            }
        let composerDetail = BridgeComposerDetail(
            composer: composers[0],
            defaultWorkId: "work-0"
        )
        let rows = works.enumerated()
            .map { index, work in
                BridgeComposerDetailRow(
                    id: Int64(index),
                    heading: index == 0 ? .works : nil,
                    item: .work(work: work, part: false)
                )
            }
        let workDetail = previewWorkDetail(work: works[0])
        let library = Library(
            composerList: { _ in
                .fixed(composers, answers: UnansweredCount())
            },
            composerDetail: {
                .fixed(answers: UnansweredCount()) { _ in composerDetail }
            },
            composerPageList: { _ in
                .fixed(rows, answers: UnansweredCount())
            },
            workDetail: {
                .fixed(answers: UnansweredCount()) { _ in workDetail }
            },
            playlists: noPlaylists
        )
        let session = LibraryBrowseSession(
            library: library,
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
                    countLine: BridgeCountLabel(
                        key: "core.library.work_count",
                        count: UInt64(2 + index % 6)
                    ),
                    linkedReleaseCount: Int64(3 + index % 4),
                    image: nil,
                )
            }
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
                        name: .named(name: "Album Title \(index + 1)"),
                        media: bridgeMediaTerms(
                            media: PreviewData.media(.vinyl, 2)
                        ),
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
            childWorkCount: 0,
            releaseCount: 3,
            trackCount: 4,
        )
    }
}
