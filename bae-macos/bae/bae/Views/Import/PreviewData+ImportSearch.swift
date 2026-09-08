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

        // MARK: - Where values were read

        /// A barcode read off the back cover, at the box the detector drew
        /// around it.
        static let backCoverBarcodeSource = BridgeValueSource(
            origin: .artwork,
            file: "Scans/back.jpg",
            region: BridgeImageRegion(x: 0.62, y: 0.78, width: 0.3, height: 0.1)
        )

        /// The same code as a CUE sheet states it.
        static let cueBarcodeSource = BridgeValueSource(
            origin: .cueSheet,
            file: "Artist Name - Album Title One.cue",
            region: nil
        )

        /// Catalog numbers extraction found and nobody has activated: one off
        /// the folder name, the rest off the artwork.
        static let catalogCandidates: [BridgeCatalogCandidate] = [
            BridgeCatalogCandidate(
                value: "LC 6006",
                sources: [
                    BridgeValueSource(
                        origin: .folderName,
                        file: nil,
                        region: nil
                    )
                ]
            ),
            BridgeCatalogCandidate(
                value: "BN-4055",
                sources: [
                    BridgeValueSource(
                        origin: .artwork,
                        file: "Scans/back.jpg",
                        region: BridgeImageRegion(
                            x: 0.1,
                            y: 0.9,
                            width: 0.2,
                            height: 0.04
                        )
                    )
                ]
            ),
            BridgeCatalogCandidate(
                value: "7243 8 29100",
                sources: [
                    BridgeValueSource(
                        origin: .artwork,
                        file: "Scans/inlay.jpg",
                        region: nil
                    )
                ]
            ),
            BridgeCatalogCandidate(
                value: "CDP 546",
                sources: [
                    BridgeValueSource(
                        origin: .textFile,
                        file: "info.txt",
                        region: nil
                    )
                ]
            ),
        ]

        /// Both providers' cells for one value, as the walks stand.
        static func cells(
            _ musicBrainz: BridgeLookupState,
            _ discogs: BridgeLookupState
        ) -> [BridgeProviderCell] {
            [
                BridgeProviderCell(source: .musicBrainz, lookup: musicBrainz),
                BridgeProviderCell(source: .discogs, lookup: discogs),
            ]
        }

        /// A lookup that named the exact-match album's pressings.
        static let foundExact = BridgeLookupState.found(
            count: 2,
            groups: [searchGroupExactBridge]
        )

        // MARK: - Typed-search runs

        /// A settled search over both providers, with results.
        static let manualSearchRun = BridgeCandidateSearch(
            query: .general(artist: "Artist Name", album: "Album Title One"),
            musicbrainz: .done(count: 3),
            discogs: .done(count: 1),
            groups: searchGroupsManualBridge,
            libraryStatuses: [:],
            status: .found
        )

        /// MusicBrainz has landed; Discogs is still out.
        static let searchRunInFlight = BridgeCandidateSearch(
            query: .general(artist: "Artist Name", album: "Album Title One"),
            musicbrainz: .done(count: 3),
            discogs: .searching,
            groups: searchGroupsManualBridge,
            libraryStatuses: [:],
            status: .searching
        )

        /// One provider answered, the other dropped.
        static let searchRunSourceFailed = BridgeCandidateSearch(
            query: .catalogNumber(catalogNumber: "WPCR-80001"),
            musicbrainz: .done(count: 1),
            discogs: .failed(failure: .network),
            groups: [searchGroupsManualBridge[0]],
            libraryStatuses: [:],
            status: .failed
        )

        /// Both providers answered with nothing.
        static let searchRunEmpty = BridgeCandidateSearch(
            query: .general(artist: "Artist Name", album: "Album Title"),
            musicbrainz: .done(count: 0),
            discogs: .done(count: 0),
            groups: [],
            libraryStatuses: [:],
            status: .noMatches
        )

        // MARK: - Pane states

        /// Find online before an automatic run starts.
        static let searchStateIdle = searchState(identifyState: .idle)

        /// Auto-lookup in progress: the disc ID has landed, Discogs has
        /// answered the first barcode while MusicBrainz is still on it, the
        /// second barcode waits, and the catalog numbers wait to be picked.
        static let identifyRunInFlight = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .read(
                discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                source: BridgeDiscIdFile(
                    kind: .log,
                    file: "Artist Name - Album Title One.log"
                ),
                lookup: .found(count: 1, groups: [searchGroupExactBridge])
            ),
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        sources: [cueBarcodeSource, backCoverBarcodeSource],
                        cells: cells(.lookingUp, foundExact)
                    ),
                    BridgeSignalValueRow(
                        value: "9999999999999",
                        sources: [
                            BridgeValueSource(
                                origin: .artwork,
                                file: "Scans/inlay.jpg",
                                region: nil
                            )
                        ],
                        cells: cells(.queued, .notAsked)
                    ),
                ]
            ),
            catalog: .numbers(
                scanning: false,
                rows: [],
                candidates: catalogCandidates
            )
        )

        /// A run that has only just started: nothing read yet, the artwork
        /// still being read for barcodes and numbers.
        static let identifyRunStarting = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .reading,
            barcode: .rows(scanning: true, rows: []),
            catalog: .numbers(scanning: true, rows: [], candidates: [])
        )

        /// No disc ID; Discogs failed the first barcode while MusicBrainz
        /// moved on to the second, and one chosen catalog number is out at
        /// MusicBrainz and empty at Discogs.
        static let identifyRunProviderFailed = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .absent,
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "5051961234567",
                        sources: [backCoverBarcodeSource],
                        cells: cells(.noMatch, .failed(failure: .timeout))
                    ),
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        sources: [cueBarcodeSource],
                        cells: cells(.lookingUp, .notAsked)
                    ),
                ]
            ),
            catalog: .numbers(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "LC 6006",
                        sources: catalogCandidates[0].sources,
                        cells: cells(.lookingUp, .noMatch)
                    )
                ],
                candidates: Array(catalogCandidates.dropFirst())
            )
        )

        /// The provider-failed run with its chosen catalog number back among
        /// the tiles — what taking it out of the run leaves.
        static let identifyRunCatalogWaiting = BridgeIdentifyRun(
            providers: identifyRunProviderFailed.providers,
            discId: identifyRunProviderFailed.discId,
            barcode: identifyRunProviderFailed.barcode,
            catalog: .numbers(
                scanning: false,
                rows: [],
                candidates: catalogCandidates
            )
        )

        /// Every lookup answered empty.
        static let identifyRunNothingFound = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .read(
                discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                source: BridgeDiscIdFile(
                    kind: .log,
                    file: "Artist Name - Album Title One.log"
                ),
                lookup: .noMatch
            ),
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        sources: [backCoverBarcodeSource],
                        cells: cells(.noMatch, .noMatch)
                    )
                ]
            ),
            catalog: .numbers(
                scanning: false,
                rows: [],
                candidates: Array(catalogCandidates.prefix(2))
            )
        )

        /// A settled run in which both signals matched: the disc ID's one
        /// release and the barcode's two, the artwork scanned clean of catalog
        /// numbers but the folder name carrying one.
        static let identifyRunFound = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .read(
                discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                source: BridgeDiscIdFile(
                    kind: .log,
                    file: "Artist Name - Album Title One.log"
                ),
                lookup: .found(count: 1, groups: [searchGroupExactBridge])
            ),
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        sources: [cueBarcodeSource, backCoverBarcodeSource],
                        cells: cells(foundExact, foundExact)
                    )
                ]
            ),
            catalog: .numbers(
                scanning: false,
                rows: [],
                candidates: Array(catalogCandidates.prefix(1))
            )
        )

        /// Nothing to look up on its own — no LOG, no CUE, no barcode — but
        /// catalog numbers a person can still activate.
        static let identifyRunAwaitingCatalog = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .absent,
            barcode: .absent,
            catalog: .numbers(
                scanning: false,
                rows: [],
                candidates: Array(catalogCandidates.prefix(2))
            )
        )

        static let searchStateTriangulating = searchState(
            identifyState: .triangulating(
                run: identifyRunInFlight,
                groups: [searchGroupExact],
                libraryStatuses: [:],
                provenance: searchProvenanceExact,
                narrowedOut: .nothing
            ),
            signals: settledSignals
        )

        /// The terminal Found verdict: one album, both sources cross-linked.
        static let searchStateFoundExact = searchState(
            identifyState: .found(
                run: identifyRunFound,
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact,
                narrowedOut: .nothing
            ),
            signals: settledSignals
        )

        /// The signals agreed on one release and each named another the
        /// agreement discarded — the disclosure's own case.
        static let searchStateNarrowedOut = searchState(
            identifyState: .found(
                run: identifyRunFound,
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact,
                narrowedOut: NarrowedOut(
                    groups: [discidOnlyGroup, barcodeOnlyGroup]
                        .map(ReleaseGroup.init(bridge:)),
                    libraryStatuses: [:],
                    provenance: disagreementProvenance
                )
            ),
            signals: settledSignals
        )

        /// The disc ID and the barcode named different albums: every one of
        /// them is offered.
        static let searchStateDisagreement = searchState(
            identifyState: IdentifyState(bridge: bridgeDisagreementState)
        )

        /// The bridge shape of the disagreement above — what a run in flight
        /// carries across, for a surface driven by the runtime signal.
        static let bridgeDisagreementState = BridgeIdentifyState.found(
            run: identifyRunFound,
            groups: [discidOnlyGroup, barcodeOnlyGroup],
            libraryStatuses: [:],
            trackCount: 11,
            provenance: disagreementProvenance,
            narrowedOut: BridgeNarrowedOut(
                groups: [],
                libraryStatuses: [:],
                provenance: [:]
            )
        )

        /// Both signals ran and neither source knew them.
        static let searchStateNotFound = searchState(
            identifyState: .notFoundAnywhere(run: identifyRunNothingFound),
            signals: settledSignals
        )

        /// The folder carries nothing to look up and nothing to offer.
        static let searchStateNoSignals = searchState(
            identifyState: .manualOnly(trackCount: 9, run: nil),
            signals: settledSignals
        )

        /// Nothing to look up on its own, but catalog numbers to activate.
        static let searchStateAwaitingCatalog = searchState(
            identifyState: .manualOnly(
                trackCount: 9,
                run: identifyRunAwaitingCatalog
            ),
            signals: settledSignals
        )

        /// One source dropped while the other's matches stand.
        static let searchStateSourceFailure = searchState(
            identifyState: .failed(
                run: identifyRunProviderFailed,
                failures: [
                    .barcode(source: .discogs, failure: .timeout)
                ],
                groups: [searchGroupExact],
                libraryStatuses: [:],
                provenance: searchProvenanceExact,
                narrowedOut: .nothing
            )
        )

        /// Nothing answered, so the reasons take the result area.
        static let searchStateAllSourcesFailed = searchState(
            identifyState: .failed(
                run: BridgeIdentifyRun(
                    providers: [.musicBrainz, .discogs],
                    discId: .read(
                        discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                        source: BridgeDiscIdFile(
                            kind: .log,
                            file: "Artist Name - Album Title One.log"
                        ),
                        lookup: .failed(failure: .network)
                    ),
                    barcode: .rows(
                        scanning: false,
                        rows: [
                            BridgeSignalValueRow(
                                value: "0123456789012",
                                sources: [backCoverBarcodeSource],
                                cells: cells(
                                    .noMatch,
                                    .failed(failure: .provider(status: 503))
                                )
                            )
                        ]
                    ),
                    catalog: .noneFound
                ),
                failures: [
                    .discId(failure: .network),
                    .barcode(source: .discogs, failure: .provider(status: 503)),
                ],
                groups: [],
                libraryStatuses: [:],
                provenance: [:],
                narrowedOut: .nothing
            )
        )

        /// A failure with no ledger to put it on: the folder carried nothing
        /// to lay out, or the verdict was stored before its signals were. The
        /// reasons are the whole pane, so the retry sits under them.
        static let searchStateFailedWithoutRun = searchState(
            identifyState: .failed(
                run: nil,
                failures: [
                    .discId(failure: .network),
                    .barcode(source: .discogs, failure: .provider(status: 503)),
                ],
                groups: [],
                libraryStatuses: [:],
                provenance: [:],
                narrowedOut: .nothing
            )
        )

        /// A sole match core is picking on its own: its row holds the
        /// spinner while its details fetch and the answer saves.
        static let searchStateFinalizing = searchState(
            identifyState: .found(
                run: identifyRunFound,
                groups: [
                    ReleaseGroup(
                        bridge: BridgeReleaseGroup(
                            id: "group-preview",
                            title: "Album Title",
                            artist: "Artist Name",
                            label: "Label Name",
                            coverArt: nil,
                            sources: searchGroupExactBridge.sources,
                            yearMin: 1996,
                            yearMax: 1996,
                            pressings: [exactPressings[1]]
                        )
                    )
                ],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact,
                narrowedOut: .nothing
            ),
            signals: settledSignals,
            isFinalizing: true
        )

        /// A typed search still running over the Found verdict.
        static let searchStateSearching = searchState(
            identifyState: .found(
                run: identifyRunFound,
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact,
                narrowedOut: .nothing
            ),
            search: searchRunInFlight,
            signals: settledSignals
        )

        /// A settled typed search over the Found verdict.
        static let searchStateManual = searchState(
            identifyState: .found(
                run: identifyRunFound,
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact,
                narrowedOut: .nothing
            ),
            search: manualSearchRun,
            signals: settledSignals
        )

        /// A typed search one source dropped, over the Found verdict.
        static let searchStateSearchFailed = searchState(
            identifyState: .found(
                run: identifyRunFound,
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact,
                narrowedOut: .nothing
            ),
            search: searchRunSourceFailed,
            signals: settledSignals
        )

        /// A typed search both sources answered with nothing.
        static let searchStateSearchEmpty = searchState(
            identifyState: .found(
                run: identifyRunFound,
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 11,
                provenance: searchProvenanceExact,
                narrowedOut: .nothing
            ),
            search: searchRunEmpty,
            signals: settledSignals
        )

        /// The pane's state with only the situation each preview is about
        /// stated; everything else is the inert default.
        static func searchState(
            identifyState: IdentifyState,
            search: BridgeCandidateSearch? = nil,
            signals: Signals? = nil,
            libraryStatuses: [String: BridgeLibraryStatus] = [:],
            selectedReleaseId: String? = nil,
            loadingReleaseId: String? = nil,
            isFinalizing: Bool = false,
        ) -> ImportSearchState {
            ImportSearchState(
                identifyState: identifyState,
                error: nil,
                search: search,
                selectedReleaseId: selectedReleaseId,
                loadingReleaseId: loadingReleaseId,
                isImporting: false,
                isFinalizing: isFinalizing,
                libraryStatuses: libraryStatuses,
                signals: signals,
                filePaths: [:]
            )
        }
    }
#endif
