import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Find online states whose words and looks the tests leave to the
    /// gallery: the Discogs bar; every shape of identify run's band; a row
    /// whose release could not be read, with its failure line and Retry; a
    /// pane nothing has run for, and one with no source to ask; a folder
    /// whose matches are pressings of one album, offered Not sure; a card
    /// whose album links could not be read; a failure with no band, its
    /// lines and Retry; the rows agreement set aside, listed above the line
    /// that puts them away.
    static let findOnlineScenes: [ShotScene] = [
        pane("import-search-not-started", PreviewData.searchStateIdle),
        ShotScene(
            id: "import-search-discogs-bar",
            size: CGSize(width: 900, height: 600)
        ) {
            let images = PreviewData.artImageStore()
            return ShotStage(
                ImportSearchPane.preview(state: PreviewData.searchStateIdle)
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            discogsUsable: false
                        )
                    )
                    .importPreviewEnvironment(images: images)
                    .windowBackground(),
                images: images
            )
        },
        ShotScene(
            id: "identify-band-runs",
            size: CGSize(width: 660, height: 900)
        ) {
            let images = PreviewData.artImageStore()
            return ShotStage(
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(bandRuns.enumerated()), id: \.offset) {
                        _,
                        run in
                        IdentifierBand(
                            run: run,
                            onToggleLookup: { _ in },
                            onToggleCatalogAgreement: { _ in },
                            onRetryFailed: {},
                            onEditTitleSearch: { _, _ in }
                        )
                        Divider()
                    }
                }
                .importPreviewEnvironment(images: images)
                .windowBackground(),
                images: images
            )
        },
        ShotScene(
            id: "import-search-no-source",
            size: CGSize(width: 900, height: 600)
        ) {
            let images = PreviewData.artImageStore()
            return ShotStage(
                ImportSearchPane.preview(state: PreviewData.searchStateIdle)
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            discogsUsable: false,
                            musicBrainz: .off
                        )
                    )
                    .importPreviewEnvironment(images: images)
                    .windowBackground(),
                images: images
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
                    unattendedPick: nil,
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
            let images = PreviewData.artImageStore()
            return ShotStage(
                ReleaseGroupCard(group: PreviewData.searchGroupLinksUnread)
                    .importPreviewEnvironment(images: images)
                    .windowBackground(),
                images: images
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
            let images = PreviewData.artImageStore()
            return ShotStage(
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
                .importPreviewEnvironment(images: images)
                .windowBackground(),
                images: images
            )
        },
        ShotScene(
            id: "import-search-row-failed",
            size: CGSize(width: 900, height: 160)
        ) {
            let images = PreviewData.artImageStore()
            return ShotStage(
                failedRow
                    .frame(width: 900, height: 160, alignment: .top)
                    .importPreviewEnvironment(images: images)
                    .background(Theme.background),
                images: images
            )
        },
    ]

    /// Each shape of run the band draws: starting, in flight, a provider
    /// failed, a catalog number waiting to be used, a barcode left out and
    /// both asked, a Disc ID left out and one no provider answers, nothing
    /// found.
    private static var bandRuns: [BridgeIdentifyRun] {
        [
            PreviewData.identifyRunStarting,
            PreviewData.identifyRunInFlight,
            PreviewData.identifyRunProviderFailed,
            PreviewData.identifyRunCatalogWaiting,
            PreviewData.identifyRunBarcodeLeftOut,
            PreviewData.identifyRunBothBarcodesAsked,
            PreviewData.identifyRunDiscIdLeftOut,
            PreviewData.identifyRunOneSource,
            PreviewData.identifyRunNothingFound,
        ]
    }

    /// The Find online pane over `state`.
    private static func pane(_ id: String, _ state: ImportSearchState)
        -> ShotScene
    {
        ShotScene(id: id, size: CGSize(width: 900, height: 600)) {
            let images = PreviewData.artImageStore()
            return ShotStage(
                ImportSearchPane.preview(state: state)
                    .importPreviewEnvironment(images: images)
                    .windowBackground(),
                images: images
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
                sources: offered.sources,
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
