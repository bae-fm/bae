#if DEBUG
    import AppKit
    import BaeKit
    import Foundation

    /// Preview fixtures for the Find online pane: its identify verdicts, its
    /// typed-search runs, and the signals behind both.
    extension PreviewData {
        // MARK: - Album cards

        /// Two pressings of one album, the later one carried by both sources —
        /// the cross-linked case the pane's row tags render.
        static let exactPressings: [BridgePressing] = [
            BridgePressing(
                releases: [
                    BridgeMetadataResult(
                        source: .musicBrainz,
                        releaseId: "rel-123",
                        year: 1988,
                        format: "CD",
                        label: "Label Name",
                        catalogNumber: "1871-2",
                        country: "US",
                        barcode: nil,
                        sourceGroupId: "group-preview"
                    )
                ],
                pick: .externalRelease(
                    source: .musicBrainz,
                    releaseId: "rel-123",
                    partners: []
                )
            ),
            BridgePressing(
                releases: [
                    BridgeMetadataResult(
                        source: .musicBrainz,
                        releaseId: "rel-456",
                        year: 1996,
                        format: "CD",
                        label: "Label Name",
                        catalogNumber: "6006-2",
                        country: "US",
                        barcode: "0123456789012",
                        sourceGroupId: "group-preview"
                    ),
                    BridgeMetadataResult(
                        source: .discogs,
                        releaseId: "rel-456-d",
                        year: 1996,
                        format: "CD, Album, Reissue",
                        label: "Label Name",
                        catalogNumber: "6006-2",
                        country: "US",
                        barcode: "0123456789012",
                        sourceGroupId: "master-6"
                    ),
                ],
                pick: .externalRelease(
                    source: .musicBrainz,
                    releaseId: "rel-456",
                    partners: [
                        BridgeMetadataRef(
                            source: .discogs,
                            releaseId: "rel-456-d"
                        )
                    ]
                )
            ),
        ]

        static let searchGroupExactBridge = BridgeReleaseGroup(
            id: "group-preview",
            title: "Album Title",
            artist: "Artist Name",
            label: "Label Name",
            coverArt: nil,
            sources: [
                BridgeReleaseGroupSource(
                    source: .musicBrainz,
                    groupUrl:
                        "https://musicbrainz.org/release-group/group-preview"
                ),
                BridgeReleaseGroupSource(
                    source: .discogs,
                    groupUrl: "https://www.discogs.com/master/master-6"
                ),
            ],
            yearMin: 1988,
            yearMax: 1996,
            pressings: exactPressings
        )

        static let searchGroupExact = ReleaseGroup(
            bridge: searchGroupExactBridge
        )

        /// The disc ID named the first pressing, the barcode the second.
        static let searchProvenanceExact: [String: BridgeResultProvenance] = [
            "rel-123": BridgeResultProvenance(
                byDiscId: true,
                byBarcode: false,
                byCatalog: false
            ),
            "rel-456": BridgeResultProvenance(
                byDiscId: false,
                byBarcode: true,
                byCatalog: false
            ),
        ]

        /// Two distinct albums — the typed-search results state.
        static let searchGroupsManualBridge: [BridgeReleaseGroup] = [
            BridgeReleaseGroup(
                id: "grp-1",
                title: "Album Title One",
                artist: "Artist Name",
                label: "Label Name",
                coverArt: nil,
                sources: [
                    BridgeReleaseGroupSource(
                        source: .musicBrainz,
                        groupUrl:
                            "https://musicbrainz.org/release-group/grp-1"
                    )
                ],
                yearMin: 1996,
                yearMax: 1996,
                pressings: [
                    BridgePressing(
                        releases: [
                            BridgeMetadataResult(
                                source: .musicBrainz,
                                releaseId: "rel-aaa",
                                year: 1996,
                                format: "CD",
                                label: "Label Name",
                                catalogNumber: "6006-2",
                                country: "US",
                                barcode: "0123456789012",
                                sourceGroupId: "grp-1"
                            )
                        ],
                        pick: .externalRelease(
                            source: .musicBrainz,
                            releaseId: "rel-aaa",
                            partners: []
                        )
                    ),
                    BridgePressing(
                        releases: [
                            BridgeMetadataResult(
                                source: .musicBrainz,
                                releaseId: "rel-bbb",
                                year: 1996,
                                format: "CD",
                                label: "Another Label",
                                catalogNumber: "AL-1234",
                                country: "JP",
                                barcode: nil,
                                sourceGroupId: "grp-1"
                            )
                        ],
                        pick: .externalRelease(
                            source: .musicBrainz,
                            releaseId: "rel-bbb",
                            partners: []
                        )
                    ),
                ]
            ),
            BridgeReleaseGroup(
                id: "grp-2",
                title: "Album Title One (Remaster)",
                artist: "Artist Name",
                label: "Reissue Records",
                coverArt: nil,
                sources: [
                    BridgeReleaseGroupSource(
                        source: .musicBrainz,
                        groupUrl:
                            "https://musicbrainz.org/release-group/grp-2"
                    ),
                    BridgeReleaseGroupSource(
                        source: .discogs,
                        groupUrl: "https://www.discogs.com/master/master-7"
                    ),
                ],
                yearMin: 2005,
                yearMax: 2005,
                pressings: [
                    BridgePressing(
                        releases: [
                            BridgeMetadataResult(
                                source: .musicBrainz,
                                releaseId: "rel-ccc",
                                year: 2005,
                                format: "CD",
                                label: "Reissue Records",
                                catalogNumber: "RR-500",
                                country: "EU",
                                barcode: "0123456789029",
                                sourceGroupId: "grp-2"
                            ),
                            BridgeMetadataResult(
                                source: .discogs,
                                releaseId: "rel-ddd",
                                year: 2005,
                                format: "CD, Album, Reissue, Remastered",
                                label: "Reissue Records",
                                catalogNumber: "RR-500",
                                country: "EU",
                                barcode: "0123456789029",
                                sourceGroupId: "master-7"
                            ),
                        ],
                        pick: .externalRelease(
                            source: .musicBrainz,
                            releaseId: "rel-ccc",
                            partners: [
                                BridgeMetadataRef(
                                    source: .discogs,
                                    releaseId: "rel-ddd"
                                )
                            ]
                        )
                    )
                ]
            ),
        ]

        static let searchGroupsManual: [ReleaseGroup] =
            searchGroupsManualBridge.map(ReleaseGroup.init(bridge:))

        /// The albums the disc ID and the barcode each named when they share
        /// none — one card per album.
        static let discidOnlyGroup = BridgeReleaseGroup(
            id: "group-disc",
            title: "Album Title",
            artist: "Artist Name",
            label: "Label A",
            coverArt: nil,
            sources: [
                BridgeReleaseGroupSource(
                    source: .musicBrainz,
                    groupUrl: "https://musicbrainz.org/release-group/group-disc"
                )
            ],
            yearMin: 1996,
            yearMax: 1996,
            pressings: [
                BridgePressing(
                    releases: [
                        BridgeMetadataResult(
                            source: .musicBrainz,
                            releaseId: "rel-disc-1",
                            year: 1996,
                            format: "CD",
                            label: "Label A",
                            catalogNumber: "AAA-001",
                            country: "US",
                            barcode: nil,
                            sourceGroupId: "group-disc"
                        )
                    ],
                    pick: .externalRelease(
                        source: .musicBrainz,
                        releaseId: "rel-disc-1",
                        partners: []
                    )
                )
            ]
        )

        static let barcodeOnlyGroup = BridgeReleaseGroup(
            id: "group-bar",
            title: "Other Album Title",
            artist: "Artist Name",
            label: "Label B",
            coverArt: nil,
            sources: [
                BridgeReleaseGroupSource(
                    source: .musicBrainz,
                    groupUrl: "https://musicbrainz.org/release-group/group-bar"
                )
            ],
            yearMin: 2001,
            yearMax: 2001,
            pressings: [
                BridgePressing(
                    releases: [
                        BridgeMetadataResult(
                            source: .musicBrainz,
                            releaseId: "rel-bar-1",
                            year: 2001,
                            format: "CD",
                            label: "Label B",
                            catalogNumber: "BBB-002",
                            country: "JP",
                            barcode: nil,
                            sourceGroupId: "group-bar"
                        )
                    ],
                    pick: .externalRelease(
                        source: .musicBrainz,
                        releaseId: "rel-bar-1",
                        partners: []
                    )
                )
            ]
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

        // MARK: - Signals

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

        /// Barcode excluded from triangulation while the disc ID matched.
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

        /// Both identity signals matched.
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

        /// Mid-run: the disc ID has landed, the barcode is still out, and the
        /// catalog is waiting to be told which number to look up.
        static let toolbarIdentifying = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 1),
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
                value: nil,
                origin: .folderName,
                state: .skipped,
                excluded: false,
                options: [
                    BridgeSignalOption(
                        value: "WPCR-80001",
                        origin: .folderName,
                        chosen: false
                    ),
                    BridgeSignalOption(
                        value: "LBL 999",
                        origin: .artwork,
                        chosen: false
                    ),
                    BridgeSignalOption(
                        value: "A2 16018",
                        origin: .textFile,
                        chosen: false
                    ),
                ]
            ),
        ])

        /// A catalog waiting to be told which of the folder's numbers to use.
        static let toolbarCatalogChoices = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 1),
                excluded: false,
                options: []
            ),
            BridgeToolbarSignal(
                kind: .catalog,
                value: nil,
                origin: .folderName,
                state: .skipped,
                excluded: false,
                options: [
                    BridgeSignalOption(
                        value: "WPCR-80001",
                        origin: .folderName,
                        chosen: false
                    ),
                    BridgeSignalOption(
                        value: "LBL 999",
                        origin: .artwork,
                        chosen: false
                    ),
                    BridgeSignalOption(
                        value: "A2 16018",
                        origin: .textFile,
                        chosen: false
                    ),
                ]
            ),
        ])

        /// Identify skipped — both identity signals have no value.
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

        /// Both signals ran and neither matched.
        static let toolbarNothingMatched = BridgeSignalsToolbar(signals: [
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

        // MARK: - Typed-search runs

        /// A settled search over both providers, with results.
        static let manualSearchRun = BridgeCandidateSearch(
            query: .general(artist: "Artist Name", album: "Album Title One"),
            musicbrainz: .done(count: 3),
            discogs: .done(count: 1),
            groups: searchGroupsManualBridge,
            libraryStatuses: [:],
            settled: true
        )

        /// MusicBrainz has landed; Discogs is still out.
        static let searchRunInFlight = BridgeCandidateSearch(
            query: .general(artist: "Artist Name", album: "Album Title One"),
            musicbrainz: .done(count: 3),
            discogs: .searching,
            groups: searchGroupsManualBridge,
            libraryStatuses: [:],
            settled: false
        )

        /// One provider answered, the other dropped.
        static let searchRunSourceFailed = BridgeCandidateSearch(
            query: .catalogNumber(catalogNumber: "WPCR-80001"),
            musicbrainz: .done(count: 1),
            discogs: .failed(failure: .network),
            groups: [searchGroupsManualBridge[0]],
            libraryStatuses: [:],
            settled: true
        )

        /// Both providers answered with nothing.
        static let searchRunEmpty = BridgeCandidateSearch(
            query: .general(artist: "Artist Name", album: "Album Title"),
            musicbrainz: .done(count: 0),
            discogs: .done(count: 0),
            groups: [],
            libraryStatuses: [:],
            settled: true
        )

        // MARK: - Pane states

        /// Find online before an automatic run starts.
        static let searchStateIdle = searchState(identifyState: .idle)

        /// Auto-lookup in progress: the disc ID has landed, Discogs has
        /// answered the barcode while MusicBrainz is still out, and the
        /// catalog waits for a pick.
        static let identifyRunInFlight = BridgeIdentifyRun(
            discId: .read(
                discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                sourceFile: "Artist Name - Album Title One.log",
                lookup: .found(count: 1)
            ),
            barcode: .lookups(
                codes: ["0123456789012", "9999999999999"],
                providers: [
                    BridgeProviderBarcodeLookup(
                        source: .musicBrainz,
                        state: .trying(
                            barcode: "0123456789012",
                            position: 1,
                            total: 2
                        )
                    ),
                    BridgeProviderBarcodeLookup(
                        source: .discogs,
                        state: .matched(barcode: "0123456789012", count: 2)
                    ),
                ]
            ),
            catalog: .unchosen(available: 3)
        )

        /// A run that has only just started: nothing read yet.
        static let identifyRunStarting = BridgeIdentifyRun(
            discId: .reading,
            barcode: .awaitingArtwork,
            catalog: .noneFound
        )

        static let searchStateTriangulating = searchState(
            identifyState: .triangulating(run: identifyRunInFlight),
            toolbar: toolbarIdentifying,
            signals: settledSignals
        )

        /// The terminal Found verdict: one album, both sources cross-linked.
        static let searchStateFoundExact = searchState(
            identifyState: .found(
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact
            ),
            toolbar: toolbarBothMatched,
            signals: settledSignals
        )

        /// The disc ID and the barcode named different albums: every one of
        /// them is offered.
        static let searchStateDisagreement = searchState(
            identifyState: IdentifyState(bridge: bridgeDisagreementState),
            toolbar: toolbarBothMatched
        )

        /// The bridge shape of the disagreement above — what a run in flight
        /// carries across, for a surface driven by the runtime signal.
        static let bridgeDisagreementState = BridgeIdentifyState.found(
            groups: [discidOnlyGroup, barcodeOnlyGroup],
            libraryStatuses: [:],
            trackCount: 11,
            provenance: disagreementProvenance
        )

        /// Both signals ran and neither source knew them.
        static let searchStateNotFound = searchState(
            identifyState: .notFoundAnywhere,
            toolbar: toolbarNothingMatched
        )

        /// The folder carries nothing to look up.
        static let searchStateNoSignals = searchState(
            identifyState: .manualOnly(trackCount: 9),
            toolbar: toolbarSkippedNoSignals
        )

        /// One source dropped while the other's matches stand.
        static let searchStateSourceFailure = searchState(
            identifyState: .failed(
                failures: [
                    .barcode(source: .discogs, failure: .timeout)
                ],
                groups: [searchGroupExact],
                libraryStatuses: [:],
                provenance: searchProvenanceExact
            ),
            toolbar: toolbarBarcodeExcluded
        )

        /// Nothing answered, so the reasons take the result area.
        static let searchStateAllSourcesFailed = searchState(
            identifyState: .failed(
                failures: [
                    .discId(failure: .network),
                    .barcode(source: .discogs, failure: .provider(status: 503)),
                ],
                groups: [],
                libraryStatuses: [:],
                provenance: [:]
            ),
            toolbar: toolbarBothRunning
        )

        /// A typed search still running over the Found verdict.
        static let searchStateSearching = searchState(
            identifyState: .found(
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact
            ),
            search: searchRunInFlight,
            toolbar: toolbarBothMatched,
            signals: settledSignals
        )

        /// A settled typed search over the Found verdict.
        static let searchStateManual = searchState(
            identifyState: .found(
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact
            ),
            search: manualSearchRun,
            toolbar: toolbarBothMatched,
            signals: settledSignals
        )

        /// A typed search one source dropped, over the Found verdict.
        static let searchStateSearchFailed = searchState(
            identifyState: .found(
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact
            ),
            search: searchRunSourceFailed,
            toolbar: toolbarBothMatched,
            signals: settledSignals
        )

        /// A typed search both sources answered with nothing.
        static let searchStateSearchEmpty = searchState(
            identifyState: .found(
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact
            ),
            search: searchRunEmpty,
            toolbar: toolbarBothMatched,
            signals: settledSignals
        )

        /// The pane's state with only the situation each preview is about
        /// stated; everything else is the inert default.
        static func searchState(
            identifyState: IdentifyState,
            search: BridgeCandidateSearch? = nil,
            toolbar: BridgeSignalsToolbar = BridgeSignalsToolbar(signals: []),
            signals: Signals? = nil,
            libraryStatuses: [String: BridgeLibraryStatus] = [:],
            selectedReleaseId: String? = nil,
            loadingReleaseId: String? = nil,
        ) -> ImportSearchState {
            ImportSearchState(
                identifyState: identifyState,
                error: nil,
                search: search,
                selectedReleaseId: selectedReleaseId,
                loadingReleaseId: loadingReleaseId,
                isImporting: false,
                libraryStatuses: libraryStatuses,
                signals: signals,
                signalsToolbar: toolbar
            )
        }
    }
#endif
