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
                        label: "Label Name",
                        catalogNumber: "1871-2",
                        facts: PreviewData.pressingFacts(
                            country: "US",
                            media: PreviewData.media(.cd)
                        ),
                        barcodes: [],
                        sourceGroupId: "group-preview"
                    )
                ],
                pick: .externalRelease(
                    record: BridgeMetadataRef(
                        catalog: .musicBrainz,
                        key: "rel-123"
                    ),
                    partners: []
                )
            ),
            BridgePressing(
                releases: [
                    BridgeMetadataResult(
                        source: .musicBrainz,
                        releaseId: "rel-456",
                        year: 1996,
                        label: "Label Name",
                        catalogNumber: "6006-2",
                        facts: PreviewData.pressingFacts(
                            country: "US",
                            media: PreviewData.media(.cd)
                        ),
                        barcodes: ["0123456789012"],
                        sourceGroupId: "group-preview"
                    ),
                    BridgeMetadataResult(
                        source: .discogs,
                        releaseId: "rel-456-d",
                        year: 1996,
                        label: "Label Name",
                        catalogNumber: "6006-2",
                        facts: PreviewData.pressingFacts(
                            country: "US",
                            media: PreviewData.media(.cd),
                            status: .promotion,
                            discogsDetails: [.reissue]
                        ),
                        barcodes: ["0123456789012"],
                        sourceGroupId: "master-6"
                    ),
                ],
                pick: .externalRelease(
                    record: BridgeMetadataRef(
                        catalog: .musicBrainz,
                        key: "rel-456"
                    ),
                    partners: [
                        BridgeMetadataRef(
                            catalog: .discogs,
                            key: "rel-456-d"
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
                        "https://musicbrainz.org/release-group/group-preview",
                    albumLinksUnread: false
                ),
                BridgeReleaseGroupSource(
                    source: .discogs,
                    groupUrl: "https://www.discogs.com/master/master-6",
                    albumLinksUnread: false
                ),
            ],
            yearMin: 1988,
            yearMax: 1996,
            sections: [
                BridgePressingSection(
                    album: nil,
                    pressings: exactPressings,
                    narrowedOut: []
                )
            ]
        )

        static let searchGroupExact = ReleaseGroup(
            bridge: searchGroupExactBridge
        )

        /// The exact album as a list shows it when its MusicBrainz page could
        /// not be read.
        static let searchGroupLinksUnread: ReleaseGroup = {
            var group = searchGroupExactBridge
            group.sources = [
                BridgeReleaseGroupSource(
                    source: .musicBrainz,
                    groupUrl:
                        "https://musicbrainz.org/release-group/group-preview",
                    albumLinksUnread: true
                )
            ]
            return ReleaseGroup(bridge: group)
        }()

        /// The disc ID named the first pressing and the folder's text states
        /// its catalog number, label and year; the barcode named the second,
        /// which the folder says nothing else about.
        static let searchAgreementsExact: [String: BridgeAgreements] = [
            "rel-123": BridgeAgreements(
                discId: true,
                barcode: false,
                catalog: true,
                label: true,
                year: true,
                country: false
            ),
            "rel-456": BridgeAgreements(
                discId: false,
                barcode: true,
                catalog: false,
                label: false,
                year: false,
                country: false
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
                            "https://musicbrainz.org/release-group/grp-1",
                        albumLinksUnread: false
                    )
                ],
                yearMin: 1996,
                yearMax: 1996,
                sections: [
                    BridgePressingSection(
                        album: nil,
                        pressings: [
                            BridgePressing(
                                releases: [
                                    BridgeMetadataResult(
                                        source: .musicBrainz,
                                        releaseId: "rel-aaa",
                                        year: 1996,
                                        label: "Label Name",
                                        catalogNumber: "6006-2",
                                        facts: PreviewData.pressingFacts(
                                            country: "US",
                                            media: PreviewData.media(.cd)
                                        ),
                                        barcodes: ["0123456789012"],
                                        sourceGroupId: "grp-1"
                                    )
                                ],
                                pick: .externalRelease(
                                    record: BridgeMetadataRef(
                                        catalog: .musicBrainz,
                                        key: "rel-aaa"
                                    ),
                                    partners: []
                                )
                            ),
                            BridgePressing(
                                releases: [
                                    BridgeMetadataResult(
                                        source: .musicBrainz,
                                        releaseId: "rel-bbb",
                                        year: 1996,
                                        label: "Another Label",
                                        catalogNumber: "AL-1234",
                                        facts: PreviewData.pressingFacts(
                                            country: "JP",
                                            media: PreviewData.media(.cd)
                                        ),
                                        barcodes: [],
                                        sourceGroupId: "grp-1"
                                    )
                                ],
                                pick: .externalRelease(
                                    record: BridgeMetadataRef(
                                        catalog: .musicBrainz,
                                        key: "rel-bbb"
                                    ),
                                    partners: []
                                )
                            ),
                        ],
                        narrowedOut: []
                    )
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
                            "https://musicbrainz.org/release-group/grp-2",
                        albumLinksUnread: false
                    ),
                    BridgeReleaseGroupSource(
                        source: .discogs,
                        groupUrl: "https://www.discogs.com/master/master-7",
                        albumLinksUnread: false
                    ),
                ],
                yearMin: 2005,
                yearMax: 2005,
                sections: [
                    BridgePressingSection(
                        album: nil,
                        pressings: [
                            BridgePressing(
                                releases: [
                                    BridgeMetadataResult(
                                        source: .musicBrainz,
                                        releaseId: "rel-ccc",
                                        year: 2005,
                                        label: "Reissue Records",
                                        catalogNumber: "RR-500",
                                        facts: PreviewData.pressingFacts(
                                            region: .europe,
                                            media: PreviewData.media(.cd)
                                        ),
                                        barcodes: ["0123456789029"],
                                        sourceGroupId: "grp-2"
                                    ),
                                    BridgeMetadataResult(
                                        source: .discogs,
                                        releaseId: "rel-ddd",
                                        year: 2005,
                                        label: "Reissue Records",
                                        catalogNumber: "RR-500",
                                        facts: PreviewData.pressingFacts(
                                            region: .europe,
                                            media: PreviewData.media(.cd),
                                            discogsDetails: [
                                                .reissue, .remastered,
                                            ]
                                        ),
                                        barcodes: ["0123456789029"],
                                        sourceGroupId: "master-7"
                                    ),
                                ],
                                pick: .externalRelease(
                                    record: BridgeMetadataRef(
                                        catalog: .musicBrainz,
                                        key: "rel-ccc"
                                    ),
                                    partners: [
                                        BridgeMetadataRef(
                                            catalog: .discogs,
                                            key: "rel-ddd"
                                        )
                                    ]
                                )
                            )
                        ],
                        narrowedOut: []
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
                    groupUrl:
                        "https://musicbrainz.org/release-group/group-disc",
                    albumLinksUnread: false
                )
            ],
            yearMin: 1996,
            yearMax: 1996,
            sections: [
                BridgePressingSection(
                    album: nil,
                    pressings: [
                        BridgePressing(
                            releases: [
                                BridgeMetadataResult(
                                    source: .musicBrainz,
                                    releaseId: "rel-disc-1",
                                    year: 1996,
                                    label: "Label A",
                                    catalogNumber: "AAA-001",
                                    facts: PreviewData.pressingFacts(
                                        country: "US",
                                        media: PreviewData.media(.cd)
                                    ),
                                    barcodes: [],
                                    sourceGroupId: "group-disc"
                                )
                            ],
                            pick: .externalRelease(
                                record: BridgeMetadataRef(
                                    catalog: .musicBrainz,
                                    key: "rel-disc-1"
                                ),
                                partners: []
                            )
                        )
                    ],
                    narrowedOut: []
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
                    groupUrl: "https://musicbrainz.org/release-group/group-bar",
                    albumLinksUnread: false
                )
            ],
            yearMin: 2001,
            yearMax: 2001,
            sections: [
                BridgePressingSection(
                    album: nil,
                    pressings: [
                        BridgePressing(
                            releases: [
                                BridgeMetadataResult(
                                    source: .musicBrainz,
                                    releaseId: "rel-bar-1",
                                    year: 2001,
                                    label: "Label B",
                                    catalogNumber: "BBB-002",
                                    facts: PreviewData.pressingFacts(
                                        country: "JP",
                                        media: PreviewData.media(.cd)
                                    ),
                                    barcodes: [],
                                    sourceGroupId: "group-bar"
                                )
                            ],
                            pick: .externalRelease(
                                record: BridgeMetadataRef(
                                    catalog: .musicBrainz,
                                    key: "rel-bar-1"
                                ),
                                partners: []
                            )
                        )
                    ],
                    narrowedOut: []
                )
            ]
        )

        /// Each row says what stands behind it — the whole of what tells the
        /// two apart once they are one list.
        static let disagreementAgreements: [String: BridgeAgreements] = [
            "rel-disc-1": BridgeAgreements(
                discId: true,
                barcode: false,
                catalog: false,
                label: false,
                year: true,
                country: false
            ),
            "rel-bar-1": BridgeAgreements(
                discId: false,
                barcode: true,
                catalog: false,
                label: false,
                year: false,
                country: false
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
            origin: .text(origin: .artwork),
            file: "Scans/back.jpg",
            region: BridgeImageRegion(x: 0.62, y: 0.78, width: 0.3, height: 0.1)
        )

        /// The same code as a CUE sheet states it.
        static let cueBarcodeSource = BridgeValueSource(
            origin: .text(origin: .cueSheet),
            file: "Artist Name - Album Title One.cue",
            region: nil
        )

        /// The numbers one of the offered releases carries and the folder
        /// states: chips that rank the list, one of them struck out.
        static let catalogAgreements: [BridgeCatalogAgreement] = [
            BridgeCatalogAgreement(value: "BST 84055", discounted: false),
            BridgeCatalogAgreement(value: "7243 8 21152 2 3", discounted: true),
        ]

        /// Catalog numbers extraction found and nobody has activated: one off
        /// the folder name, the rest off the artwork.
        static let catalogCandidates: [BridgeCatalogCandidate] = [
            BridgeCatalogCandidate(
                value: "LC 6006",
                sources: [
                    BridgeValueSource(
                        origin: .text(origin: .folderName),
                        file: nil,
                        region: nil
                    )
                ]
            ),
            BridgeCatalogCandidate(
                value: "BN-4055",
                sources: [
                    BridgeValueSource(
                        origin: .text(origin: .artwork),
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
                        origin: .text(origin: .artwork),
                        file: "Scans/inlay.jpg",
                        region: nil
                    )
                ]
            ),
            BridgeCatalogCandidate(
                value: "CDP 546",
                sources: [
                    BridgeValueSource(
                        origin: .text(origin: .textFile),
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

        /// The two sources' parts of a search, in core's order.
        static func searchSources(
            musicbrainz: BridgeSourceSearch,
            discogs: BridgeSourceSearch
        ) -> [BridgeSourceSearchEntry] {
            [
                BridgeSourceSearchEntry(
                    source: .musicBrainz,
                    state: musicbrainz
                ),
                BridgeSourceSearchEntry(source: .discogs, state: discogs),
            ]
        }

        /// A settled search over both providers, with results.
        static let manualSearchRun = BridgeCandidateSearch(
            query: .general(artist: "Artist Name", album: "Album Title One"),
            sources: searchSources(
                musicbrainz: .done(count: 3),
                discogs: .done(count: 1)
            ),
            groups: searchGroupsManualBridge,
            libraryStatuses: [:],
            status: .found
        )

        /// MusicBrainz has landed; Discogs is still out.
        static let searchRunInFlight = BridgeCandidateSearch(
            query: .general(artist: "Artist Name", album: "Album Title One"),
            sources: searchSources(
                musicbrainz: .done(count: 3),
                discogs: .searching
            ),
            groups: searchGroupsManualBridge,
            libraryStatuses: [:],
            status: .searching
        )

        /// One provider answered, the other dropped.
        static let searchRunSourceFailed = BridgeCandidateSearch(
            query: .catalogNumber(catalogNumber: "WPCR-80001"),
            sources: searchSources(
                musicbrainz: .done(count: 1),
                discogs: .failed(failure: .network)
            ),
            groups: [searchGroupsManualBridge[0]],
            libraryStatuses: [:],
            status: .failed
        )

        /// Both providers answered with nothing.
        static let searchRunEmpty = BridgeCandidateSearch(
            query: .general(artist: "Artist Name", album: "Album Title"),
            sources: searchSources(
                musicbrainz: .done(count: 0),
                discogs: .done(count: 0)
            ),
            groups: [],
            libraryStatuses: [:],
            status: .noMatches
        )

        // MARK: - Pane states

        /// Find online before an automatic run starts.
        static let searchStateIdle = searchState(identifyState: .idle)

        static let searchStateTriangulating = searchState(
            identifyState: .triangulating(
                run: identifyRunInFlight,
                groups: [searchGroupExact],
                libraryStatuses: [:],
                agreements: searchAgreementsExact,
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
                agreements: searchAgreementsExact,
                narrowedOut: .nothing,
                catalogAgreements: catalogAgreements
            ),
            signals: settledSignals
        )

        /// `group` as a run lists it when agreement set every one of its rows
        /// aside.
        static func setAside(_ group: BridgeReleaseGroup) -> BridgeReleaseGroup
        {
            var group = group
            group.sections = group.sections.map { section in
                BridgePressingSection(
                    album: section.album,
                    pressings: [],
                    narrowedOut: section.pressings + section.narrowedOut
                )
            }
            return group
        }

        /// The exact album with its earlier pressing set aside: the matches'
        /// own card, holding a row behind the disclosure.
        static let searchGroupExactWithSetAside: ReleaseGroup = {
            var group = searchGroupExactBridge
            group.sections = [
                BridgePressingSection(
                    album: nil,
                    pressings: [exactPressings[1]],
                    narrowedOut: [exactPressings[0]]
                )
            ]
            return ReleaseGroup(bridge: group)
        }()

        /// The signals agreed on one release. Agreement set aside another
        /// pressing of the same album, which stays on its card, and each named
        /// an album of its own — the disclosure's own case.
        static let searchStateNarrowedOut = searchState(
            identifyState: .found(
                run: identifyRunFound,
                groups: [searchGroupExactWithSetAside],
                libraryStatuses: [:],
                trackCount: 11,
                agreements: searchAgreementsExact.merging(
                    disagreementAgreements
                ) { offered, _ in offered },
                narrowedOut: NarrowedOut(
                    groups: [discidOnlyGroup, barcodeOnlyGroup]
                        .map(setAside)
                        .map(ReleaseGroup.init(bridge:)),
                    count: 3
                ),
                catalogAgreements: catalogAgreements
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
            agreements: disagreementAgreements,
            narrowedOut: BridgeNarrowedOut(groups: [], count: 0),
            catalogAgreements: catalogAgreements
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
                agreements: searchAgreementsExact,
                narrowedOut: .nothing,
                catalogAgreements: catalogAgreements
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
                                excluded: false,
                                cells: cells(
                                    .noMatch,
                                    .failed(failure: .provider(status: 503))
                                )
                            )
                        ]
                    ),
                    catalog: .noneFound,
                    search: .notNeeded
                ),
                failures: [
                    .discId(failure: .network),
                    .barcode(source: .discogs, failure: .provider(status: 503)),
                ],
                groups: [],
                libraryStatuses: [:],
                agreements: [:],
                narrowedOut: .nothing,
                catalogAgreements: []
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
                agreements: [:],
                narrowedOut: .nothing,
                catalogAgreements: []
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
                            sections: [
                                BridgePressingSection(
                                    album: nil,
                                    pressings: [exactPressings[1]],
                                    narrowedOut: []
                                )
                            ]
                        )
                    )
                ],
                libraryStatuses: [:],
                trackCount: 11,
                agreements: searchAgreementsExact,
                narrowedOut: .nothing,
                catalogAgreements: catalogAgreements
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
                agreements: searchAgreementsExact,
                narrowedOut: .nothing,
                catalogAgreements: catalogAgreements
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
                agreements: searchAgreementsExact,
                narrowedOut: .nothing,
                catalogAgreements: catalogAgreements
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
                agreements: searchAgreementsExact,
                narrowedOut: .nothing,
                catalogAgreements: catalogAgreements
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
                agreements: searchAgreementsExact,
                narrowedOut: .nothing,
                catalogAgreements: catalogAgreements
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
            )
        }
    }
#endif
