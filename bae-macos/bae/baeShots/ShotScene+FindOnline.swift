import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Find online states whose words the tests leave to the gallery: a row
    /// whose release could not be read, with its failure line and Retry; a
    /// pane nothing has run for, and one with no source to ask; a folder
    /// whose matches are pressings of one album, offered Not sure; a card
    /// whose album links could not be read; a failure with no band, its
    /// lines and Retry; the rows agreement set aside, listed above the line
    /// that puts them away.
    static let findOnlineScenes: [ShotScene] = [
        pane("import-search-not-started", PreviewData.searchStateIdle),
        ShotScene(
            id: "import-search-no-source",
            size: CGSize(width: 900, height: 600)
        ) {
            AnyView(
                ImportSearchPane.preview(state: PreviewData.searchStateIdle)
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            discogsUsable: false,
                            musicBrainz: .off
                        )
                    )
                    .importPreviewEnvironment()
                    .windowBackground()
            )
        },
        pane(
            "import-search-not-sure",
            PreviewData.searchState(
                identifyState: .found(
                    run: PreviewData.identifyRunFound,
                    groups: [PreviewData.searchGroupExact],
                    trackCount: 11,
                    agreements: PreviewData.searchAgreementsExact,
                    narrowedOutCount: 0,
                    folderCheck: nil,
                    offersSharedAlbum: true
                ),
                signals: PreviewData.settledSignals,
                needsYou: .matches(count: 2)
            )
        ),
        ShotScene(
            id: "import-search-card-links-unread",
            size: CGSize(width: 660, height: 160)
        ) {
            AnyView(
                ReleaseGroupCard(group: PreviewData.searchGroupLinksUnread)
                    .importPreviewEnvironment()
                    .windowBackground()
            )
        },
        pane(
            "import-search-failure-lines",
            PreviewData.searchStateFailedWithoutRun
        ),
        ShotScene(
            id: "import-search-narrowed-out-open",
            size: CGSize(width: 660, height: 1_100)
        ) {
            AnyView(
                FindOnlineAutomaticSection(
                    state: PreviewData.searchStateNarrowedOut,
                    onOpenSettings: {},
                    onToggleLookup: { _ in },
                    onToggleCatalogAgreement: { _ in },
                    onRetryFailed: {},
                    onEditTitleSearch: { _, _ in },
                    onSelect: { _ in },
                    onSearchManually: {},
                    onCancelIdentification: nil,
                    onKeepOwnDraft: nil,
                    onLinkSharedAlbum: nil,
                    narrowedOutExpanded: .constant(true)
                )
                .importPreviewEnvironment()
                .windowBackground()
            )
        },
        ShotScene(
            id: "import-search-row-failed",
            size: CGSize(width: 900, height: 160)
        ) {
            AnyView(
                failedRow
                    .frame(width: 900, height: 160, alignment: .top)
                    .importPreviewEnvironment()
                    .background(Theme.background)
            )
        },
    ]

    /// The Find online pane over `state`.
    private static func pane(_ id: String, _ state: ImportSearchState)
        -> ShotScene
    {
        ShotScene(id: id, size: CGSize(width: 900, height: 600)) {
            AnyView(
                ImportSearchPane.preview(state: state)
                    .importPreviewEnvironment()
                    .windowBackground()
            )
        }
    }

    /// The first exact match, its documents unread.
    @ViewBuilder
    private static var failedRow: some View {
        let offered = PreviewData.searchGroupExact.pressings[0]
        if let pressing = Pressing(
            bridge: BridgePressing(
                releases: offered.releases,
                labels: offered.labels,
                documentFailure: .timeout,
                pick: offered.link
            )
        ) {
            ImportSearchResultRow(
                pressing: pressing,
                isImporting: false,
                libraryStatus: nil,
                isSelected: false,
                onRetryUnread: {},
                onSelect: { _ in }
            )
        }
    }
}
