import BaeKit
import SwiftUI

@testable import bae

// Fixtures for the library browse views: a composer's summary and detail,
// a composer page of many works, and a `LibraryBrowseSession` builder.
// These fixtures use only generic placeholder names.
extension PreviewData {
    // MARK: - Composers

    static let composerSummary = BridgeComposerSummary(
        artistId: "composer-0",
        name: "Composer Name",
        countLine: BridgeCountLabel(
            key: "core.library.work_count",
            count: 6
        ),
        linkedReleaseCount: 4,
        image: nil,
    )

    static let composerDetail = BridgeComposerDetail(
        composer: composerSummary,
        defaultWorkId: "work-0",
    )

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
                            composerNames: "Composer Name",
                            linkedReleaseCount: 1,
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
            queries: .fixed(rows, answers: UnansweredCount()),
            onError: { _ in },
        )
    }

    // MARK: - Session

    /// A `LibraryBrowseSession` over `Library.stub()` — the detail-pane
    /// shots drive their content through `paneDetail`/`detail` props,
    /// so the session only supplies the selection state the panes read.
    @MainActor
    static func browseSession(uiStore: UiStore) -> LibraryBrowseSession {
        LibraryBrowseSession(library: .stub(), uiStore: uiStore)
    }
}

extension PreviewData {
    /// A catalog cover's selection and image point to the same image set.
    static func catalogCoverChoice(
        _ image: BridgeRemoteImageSet,
        source: BridgeCatalog
    ) -> BridgeCoverChoice {
        BridgeCoverChoice(
            selection: .remoteCover(
                selection: BridgeRemoteCoverSelection(
                    image: image,
                    source: source
                )
            ),
            image: .remote(image: image)
        )
    }
}
