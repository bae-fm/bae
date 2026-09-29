#if DEBUG
    import AppKit
    import BaeKit
    import Foundation

    /// Preview fixtures for the Find online pane: its identify verdicts, its
    /// typed-search runs, and the signals behind both.
    extension PreviewData {
        // MARK: - Album cards

        /// Two pressings of one album, the later one carried by both sources.
        static let exactPressings: [BridgePressing] = [
            BridgePressing(
                releases: [
                    BridgeMetadataResult(
                        source: .musicBrainz,
                        releaseId: "rel-123",
                        year: 1988,
                        facts: PreviewData.pressingFacts(
                            country: "US",
                            media: PreviewData.media(.cd)
                        ),
                        barcodes: [],
                        sourceGroupId: "group-preview"
                    )
                ],
                labels: [
                    BridgeLabelLine(
                        names: ["Label Name"],
                        catalogNumbers: ["1871-2"]
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
                labels: [
                    BridgeLabelLine(
                        names: ["Label Name"],
                        catalogNumbers: ["6006-2"]
                    )
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

        /// The exact album when its MusicBrainz page could not be read.
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

        /// What the folder's text agrees with about each exact-album pressing.
        static let searchAgreementsExact: [String: BridgeAgreements] = [
            "rel-123": BridgeAgreements(
                discId: true,
                barcode: false,
                catalog: true,
                label: true,
                year: true,
                country: false,
                notes: nil
            ),
            "rel-456": BridgeAgreements(
                discId: false,
                barcode: true,
                catalog: false,
                label: false,
                year: false,
                country: false,
                notes: nil
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
                                        facts: PreviewData.pressingFacts(
                                            country: "US",
                                            media: PreviewData.media(.cd)
                                        ),
                                        barcodes: ["0123456789012"],
                                        sourceGroupId: "grp-1"
                                    )
                                ],
                                labels: [
                                    BridgeLabelLine(
                                        names: ["Label Name"],
                                        catalogNumbers: ["6006-2"]
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
                                        facts: PreviewData.pressingFacts(
                                            country: "JP",
                                            media: PreviewData.media(.cd)
                                        ),
                                        barcodes: [],
                                        sourceGroupId: "grp-1"
                                    )
                                ],
                                labels: [
                                    BridgeLabelLine(
                                        names: ["Another Label"],
                                        catalogNumbers: ["AL-1234"]
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
                                labels: [
                                    BridgeLabelLine(
                                        names: ["Reissue Records"],
                                        catalogNumbers: ["RR-500"]
                                    )
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

        /// One card per album when the disc ID and the barcode share none.
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
                                    facts: PreviewData.pressingFacts(
                                        country: "US",
                                        media: PreviewData.media(.cd)
                                    ),
                                    barcodes: [],
                                    sourceGroupId: "group-disc"
                                )
                            ],
                            labels: [
                                BridgeLabelLine(
                                    names: ["Label A"],
                                    catalogNumbers: ["AAA-001"]
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
                                    facts: PreviewData.pressingFacts(
                                        country: "JP",
                                        media: PreviewData.media(.cd)
                                    ),
                                    barcodes: [],
                                    sourceGroupId: "group-bar"
                                )
                            ],
                            labels: [
                                BridgeLabelLine(
                                    names: ["Label B"],
                                    catalogNumbers: ["BBB-002"]
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

        /// What each disagreeing row's lookups and text agree with.
        static let disagreementAgreements: [String: BridgeAgreements] = [
            "rel-disc-1": BridgeAgreements(
                discId: true,
                barcode: false,
                catalog: false,
                label: false,
                year: true,
                country: false,
                notes: nil
            ),
            "rel-bar-1": BridgeAgreements(
                discId: false,
                barcode: true,
                catalog: false,
                label: false,
                year: false,
                country: false,
                notes: nil
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

        /// Catalog-number chips that rank the list, one of them struck out.
        static let catalogAgreements: [BridgeCatalogAgreement] = [
            BridgeCatalogAgreement(value: "BST 84055", discounted: false),
            BridgeCatalogAgreement(value: "7243 8 21152 2 3", discounted: true),
        ]

        /// Catalog numbers extraction found and nobody has activated.
        static let catalogCandidates: [BridgeCatalogCandidate] = [
            BridgeCatalogCandidate(value: "LC 6006"),
            BridgeCatalogCandidate(value: "BN-4055"),
            BridgeCatalogCandidate(value: "7243 8 29100"),
            BridgeCatalogCandidate(value: "CDP 546"),
        ]

        /// Both providers' cells for one value.
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
            unusableBarcode: nil,
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
            unusableBarcode: nil,
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
            unusableBarcode: nil,
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
            unusableBarcode: nil,
            sources: searchSources(
                musicbrainz: .done(count: 0),
                discogs: .done(count: 0)
            ),
            groups: [],
            libraryStatuses: [:],
            status: .noMatches
        )

        /// A shop's price-sticker number asked for as a barcode: neither
        /// provider knows it.
        static let searchRunStoreInternalBarcode = BridgeCandidateSearch(
            query: .barcode(barcode: "2100000123457"),
            unusableBarcode: .storeInternal,
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
                narrowedOutCount: 0
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
                narrowedOutCount: 0,
                catalogAgreements: catalogAgreements,
                folderCheck: nil,
                picksUnattended: false
            ),
            signals: settledSignals
        )

        /// `group` with every row set aside.
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

        /// The exact album with its earlier pressing set aside.
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

        /// One release agreed on, with other pressings and albums set aside.
        static let searchStateNarrowedOut = searchState(
            identifyState: .found(
                run: identifyRunFound,
                groups: [searchGroupExactWithSetAside]
                    + [discidOnlyGroup, barcodeOnlyGroup]
                    .map(setAside)
                    .map(ReleaseGroup.init(bridge:)),
                libraryStatuses: [:],
                trackCount: 11,
                agreements: searchAgreementsExact.merging(
                    disagreementAgreements
                ) { offered, _ in offered },
                narrowedOutCount: 3,
                catalogAgreements: catalogAgreements,
                folderCheck: nil,
                picksUnattended: false
            ),
            signals: settledSignals
        )

        /// The disc ID and the barcode named different albums, all offered.
        static let searchStateDisagreement = searchState(
            identifyState: IdentifyState(bridge: bridgeDisagreementState)
        )

        /// The bridge shape of the disagreement above.
        static let bridgeDisagreementState = BridgeIdentifyState.found(
            run: identifyRunFound,
            groups: [discidOnlyGroup, barcodeOnlyGroup],
            libraryStatuses: [:],
            trackCount: 11,
            agreements: disagreementAgreements,
            narrowedOutCount: 0,
            catalogAgreements: catalogAgreements,
            folderCheck: nil,
            picksUnattended: false
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
                narrowedOutCount: 0,
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
                        lookup: .failed(failure: .network)
                    ),
                    barcode: .rows(
                        scanning: false,
                        rows: [
                            BridgeSignalValueRow(
                                value: "0123456789012",
                                excluded: false,
                                cells: cells(
                                    .noMatch,
                                    .failed(failure: .provider(status: 503))
                                )
                            )
                        ]
                    ),
                    catalog: .noneFound,
                    isrc: .absent,
                    search: .notNeeded
                ),
                failures: [
                    .discId(failure: .network),
                    .barcode(source: .discogs, failure: .provider(status: 503)),
                ],
                groups: [],
                libraryStatuses: [:],
                agreements: [:],
                narrowedOutCount: 0,
                catalogAgreements: []
            )
        )

        /// A failure with no run to show, so the reasons fill the pane.
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
                narrowedOutCount: 0,
                catalogAgreements: []
            )
        )

        /// A sole match being picked automatically, its row spinning.
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
                narrowedOutCount: 0,
                catalogAgreements: catalogAgreements,
                folderCheck: nil,
                picksUnattended: true
            ),
            signals: settledSignals,
            isFinalizing: true
        )

        /// A sole match whose tracklist does not fit the folder: offered, not
        /// picked, with the check it failed under its row.
        static let searchStateSoleUnfit = searchState(
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
                trackCount: 13,
                agreements: searchAgreementsExact,
                narrowedOutCount: 0,
                catalogAgreements: catalogAgreements,
                folderCheck: .trackCountDisagrees(local: 13, source: 12),
                picksUnattended: false
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
                narrowedOutCount: 0,
                catalogAgreements: catalogAgreements,
                folderCheck: nil,
                picksUnattended: false
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
                narrowedOutCount: 0,
                catalogAgreements: catalogAgreements,
                folderCheck: nil,
                picksUnattended: false
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
                narrowedOutCount: 0,
                catalogAgreements: catalogAgreements,
                folderCheck: nil,
                picksUnattended: false
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
                narrowedOutCount: 0,
                catalogAgreements: catalogAgreements,
                folderCheck: nil,
                picksUnattended: false
            ),
            search: searchRunEmpty,
            signals: settledSignals
        )

        /// The pane's state with everything but the given parts left default.
        static func searchState(
            identifyState: IdentifyState,
            search: BridgeCandidateSearch? = nil,
            signals: Signals? = nil,
            libraryStatuses: [String: BridgeLibraryStatus] = [:],
            selectedReleaseId: String? = nil,
            loadingReleaseId: String? = nil,
            isFinalizing: Bool = false,
            needsYou: BridgeNeedsYouReason? = nil,
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
                needsYou: needsYou,
            )
        }
    }
#endif
