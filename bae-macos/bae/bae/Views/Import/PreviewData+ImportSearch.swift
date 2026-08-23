#if DEBUG
    import AppKit
    import BaeKit
    import Foundation

    /// Preview fixtures for the import identity search states and signal toolbar.
    extension PreviewData {
        // MARK: - Import search

        /// Two pressings of one release group — the exact-match results state.
        static let exactPressings: [BridgeMetadataResult] = [
            BridgeMetadataResult(
                source: .musicBrainz,
                releaseId: "rel-123",
                year: 1996,
                format: "CD",
                label: "Label Name",
                catalogNumber: "6006-2",
                country: "US"
            ),
            BridgeMetadataResult(
                source: .musicBrainz,
                releaseId: "rel-456",
                year: 1988,
                format: "CD",
                label: "Label Name",
                catalogNumber: "1871-2",
                country: "US"
            ),
        ]

        static let searchGroupExact = ReleaseGroup(
            bridge: BridgeReleaseGroup(
                id: "group-preview",
                sourceGroupId: "group-preview",
                title: "Album Title",
                artist: "Artist Name",
                coverArt: nil,
                sourceLabel: "MusicBrainz",
                groupUrl: "https://musicbrainz.org/release-group/group-preview",
                yearMin: 1988,
                yearMax: 1996,
                pressings: exactPressings
            )
        )

        static let searchProvenanceExact: [String: BridgeResultProvenance] =
            Dictionary(
                uniqueKeysWithValues: exactPressings.map {
                    (
                        $0.releaseId,
                        BridgeResultProvenance(
                            byDiscId: true,
                            byBarcode: false,
                            byCatalog: true
                        )
                    )
                }
            )

        /// Two distinct release groups — the manual-search results state.
        static let searchGroupsManual: [ReleaseGroup] = [
            ReleaseGroup(
                bridge: BridgeReleaseGroup(
                    id: "grp-1",
                    sourceGroupId: "grp-1",
                    title: "Album Title One",
                    artist: "Artist Name",
                    coverArt: nil,
                    sourceLabel: "MusicBrainz",
                    groupUrl: "https://musicbrainz.org/release-group/grp-1",
                    yearMin: 1996,
                    yearMax: 1996,
                    pressings: [
                        BridgeMetadataResult(
                            source: .musicBrainz,
                            releaseId: "rel-aaa",
                            year: 1996,
                            format: "CD",
                            label: "Label Name",
                            catalogNumber: "6006-2",
                            country: "US"
                        ),
                        BridgeMetadataResult(
                            source: .musicBrainz,
                            releaseId: "rel-bbb",
                            year: 1996,
                            format: "CD",
                            label: "Another Label",
                            catalogNumber: "AL-1234",
                            country: "JP"
                        ),
                    ]
                )
            ),
            ReleaseGroup(
                bridge: BridgeReleaseGroup(
                    id: "grp-2",
                    sourceGroupId: "grp-2",
                    title: "Album Title One (Remaster)",
                    artist: "Artist Name",
                    coverArt: nil,
                    sourceLabel: "MusicBrainz",
                    groupUrl: "https://musicbrainz.org/release-group/grp-2",
                    yearMin: 2005,
                    yearMax: 2005,
                    pressings: [
                        BridgeMetadataResult(
                            source: .musicBrainz,
                            releaseId: "rel-ccc",
                            year: 2005,
                            format: "CD",
                            label: "Reissue Records",
                            catalogNumber: "RR-500",
                            country: "EU"
                        )
                    ]
                )
            ),
        ]

        /// The releases the disc ID and the barcode each named when they share
        /// none — one card per release group.
        static let discidOnlyResults: [BridgeMetadataResult] = [
            BridgeMetadataResult(
                source: .musicBrainz,
                releaseId: "rel-disc-1",
                year: 1996,
                format: "CD",
                label: "Label A",
                catalogNumber: "AAA-001",
                country: "US"
            )
        ]

        static let barcodeOnlyResults: [BridgeMetadataResult] = [
            BridgeMetadataResult(
                source: .musicBrainz,
                releaseId: "rel-bar-1",
                year: 2001,
                format: "CD",
                label: "Label B",
                catalogNumber: "BBB-002",
                country: "JP"
            )
        ]

        static let discidOnlyGroup = BridgeReleaseGroup(
            id: "group-disc",
            sourceGroupId: "group-disc",
            title: "Album Title",
            artist: "Artist Name",
            coverArt: nil,
            sourceLabel: "MusicBrainz",
            groupUrl: "https://musicbrainz.org/release-group/group-disc",
            yearMin: 1996,
            yearMax: 1996,
            pressings: discidOnlyResults
        )

        static let barcodeOnlyGroup = BridgeReleaseGroup(
            id: "group-bar",
            sourceGroupId: "group-bar",
            title: "Other Album Title",
            artist: "Artist Name",
            coverArt: nil,
            sourceLabel: "MusicBrainz",
            groupUrl: "https://musicbrainz.org/release-group/group-bar",
            yearMin: 2001,
            yearMax: 2001,
            pressings: barcodeOnlyResults
        )

        /// Each row says which signal produced it — the whole of what tells
        /// the two apart once they are one list.
        static let disagreementProvenance: [String: BridgeResultProvenance] = [
            "rel-disc-1": BridgeResultProvenance(
                byDiscId: true,
                byBarcode: false,
                byCatalog: false
            ),
            "rel-bar-1": BridgeResultProvenance(
                byDiscId: false,
                byBarcode: true,
                byCatalog: false
            ),
        ]

        /// Settled OCR/text signals — catalogs plus cover free-text.
        static let settledSignals = Signals(
            text: .settled(
                catalogs: ["WPCR-80001"],
                freeText: [
                    "Artist Name",
                    "Album Title",
                    "Label Records JP - WPCR-80001",
                    "Recorded at Studio A",
                    "Produced by Producer Name",
                ]
            )
        )

        // MARK: - Signals toolbar

        /// Both identity signals still looking up, one catalog filter present.
        static let toolbarBothRunning = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .lookingUp,
                excluded: false,
                options: []
            ),
            BridgeToolbarSignal(
                kind: .barcode,
                value: "0123456789012",
                origin: .artwork,
                state: .lookingUp,
                excluded: false,
                options: []
            ),
            BridgeToolbarSignal(
                kind: .catalog,
                value: "WPCR-80001",
                origin: .folderName,
                state: .noMatch,
                excluded: false,
                options: []
            ),
        ])

        /// Disc-id settled, barcode still running, two catalog filters (one
        /// confirming, one not).
        static let toolbarOneSettled = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 3),
                excluded: false,
                options: []
            ),
            BridgeToolbarSignal(
                kind: .barcode,
                value: "0123456789012",
                origin: .artwork,
                state: .lookingUp,
                excluded: false,
                options: []
            ),
            BridgeToolbarSignal(
                kind: .catalog,
                value: "WPCR-80001",
                origin: .folderName,
                state: .found(count: 1),
                excluded: false,
                options: []
            ),
            BridgeToolbarSignal(
                kind: .catalog,
                value: "A2 16018",
                origin: .textFile,
                state: .noMatch,
                excluded: false,
                options: []
            ),
        ])

        /// Barcode excluded from triangulation while both identity signals matched.
        static let toolbarBarcodeExcluded = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 2),
                excluded: false,
                options: []
            ),
            BridgeToolbarSignal(
                kind: .barcode,
                value: "0123456789012",
                origin: .artwork,
                state: .found(count: 4),
                excluded: true,
                options: []
            ),
            BridgeToolbarSignal(
                kind: .catalog,
                value: "WPCR-80001",
                origin: .folderName,
                state: .noMatch,
                excluded: false,
                options: []
            ),
        ])

        /// Both identity signals matched, on different releases.
        static let toolbarBothMatched = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 2),
                excluded: false,
                options: []
            ),
            BridgeToolbarSignal(
                kind: .barcode,
                value: "5051961234567",
                origin: .artwork,
                state: .found(count: 3),
                excluded: false,
                options: []
            ),
        ])

        /// Identify skipped — both identity signals have no value and are skipped.
        static let toolbarSkippedNoSignals = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                value: nil,
                origin: .discToc,
                state: .skipped,
                excluded: false,
                options: []
            ),
            BridgeToolbarSignal(
                kind: .barcode,
                value: nil,
                origin: .artwork,
                state: .skipped,
                excluded: false,
                options: []
            ),
        ])

        /// Exact-match display state: the disc ID and the chosen catalog number both
        /// name the same group.
        static let searchStateFoundExact = ImportSearchState(
            identifyState: .found(
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 0,
                provenance: searchProvenanceExact
            ),
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: true,
            signals: settledSignals,
            signalsToolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .found(count: 3),
                    excluded: false,
                    options: []
                ),
                BridgeToolbarSignal(
                    kind: .catalog,
                    value: "WPCR-80001",
                    origin: .folderName,
                    state: .found(count: 1),
                    excluded: false,
                    options: []
                ),
            ])
        )

        /// Manual-search display state: results listed, the form open.
        static let searchStateManual = ImportSearchState(
            identifyState: .found(
                groups: [searchGroupsManual[0]],
                libraryStatuses: [:],
                trackCount: 0,
                provenance: [:]
            ),
            error: nil,
            searchGroups: searchGroupsManual,
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: true,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: true,
            signals: settledSignals,
            signalsToolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .found(count: 2),
                    excluded: false,
                    options: []
                ),
                BridgeToolbarSignal(
                    kind: .catalog,
                    value: "WPCR-80001",
                    origin: .folderName,
                    state: .noMatch,
                    excluded: false,
                    options: []
                ),
            ])
        )

        /// The bridge shape of the disagreement below — what a run in flight
        /// carries across, for a surface driven by the runtime signal. The two
        /// signals named different releases, so the set is their union and the
        /// cards are one per release group.
        static let bridgeDisagreementState = BridgeIdentifyState.found(
            groups: [discidOnlyGroup, barcodeOnlyGroup],
            libraryStatuses: [:],
            trackCount: 11,
            provenance: disagreementProvenance
        )

        /// Display state where the disc ID and the barcode named different
        /// releases: every one of them is offered.
        static let searchStateDisagreement = ImportSearchState(
            identifyState: IdentifyState(bridge: bridgeDisagreementState),
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: true,
            signals: nil,
            signalsToolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .found(count: 2),
                    excluded: false,
                    options: []
                ),
                BridgeToolbarSignal(
                    kind: .barcode,
                    value: "5051961234567",
                    origin: .artwork,
                    state: .found(count: 3),
                    excluded: false,
                    options: []
                ),
            ])
        )

        /// Auto-lookup in progress: disc-id looking up, barcode skipped.
        static let searchStateTriangulating = ImportSearchState(
            identifyState: .triangulating(
                discid: .lookingUp,
                barcode: .skipped
            ),
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: false,
            signals: nil,
            signalsToolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .lookingUp,
                    excluded: false,
                    options: []
                ),
                BridgeToolbarSignal(
                    kind: .barcode,
                    value: nil,
                    origin: .artwork,
                    state: .skipped,
                    excluded: false,
                    options: []
                ),
            ])
        )

        /// Manual search after both signals came up empty.
        static let searchStateNotFound = ImportSearchState(
            identifyState: .notFoundAnywhere,
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: true,
            signals: nil,
            signalsToolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .noMatch,
                    excluded: false,
                    options: []
                ),
                BridgeToolbarSignal(
                    kind: .barcode,
                    value: "5051961234567",
                    origin: .artwork,
                    state: .noMatch,
                    excluded: false,
                    options: []
                ),
            ])
        )
    }
#endif
