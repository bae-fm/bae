#if DEBUG
    import BaeKit
    import Foundation

    /// Preview fixtures for the band an identify run draws: one
    /// `BridgeIdentifyRun` per shape a run reaches — in flight, settled,
    /// failed, and with an identifier the person left out of it. The signals
    /// they are built from, and the pane states that carry them, are in
    /// `PreviewData+ImportSearch`.
    extension PreviewData {
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
                        excluded: false,
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
                        excluded: false,
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

        /// MusicBrainz switched off, so the run asks Discogs alone: one column,
        /// one rail, and a disc ID nothing looked up — disc IDs are a
        /// MusicBrainz identifier, so with it unasked the value stands with no
        /// count beside it.
        static let identifyRunOneSource = BridgeIdentifyRun(
            providers: [.discogs],
            discId: .readNotAsked(
                discId: "aB7cD9eFgH2iJkL3mN4oP5qR6sT=",
                source: BridgeDiscIdFile(
                    kind: .log,
                    file: "Artist Name - Album Title One.log"
                )
            ),
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "5051961234567",
                        sources: [backCoverBarcodeSource],
                        excluded: false,
                        cells: [
                            BridgeProviderCell(
                                source: .discogs,
                                lookup: .lookingUp
                            )
                        ]
                    )
                ]
            ),
            catalog: .numbers(scanning: false, rows: [], candidates: [])
        )

        /// The one-source run with its disc ID taken out of the run instead
        /// of unasked for want of a provider: the same value, the off chip,
        /// and the way back.
        static let identifyRunDiscIdLeftOut = BridgeIdentifyRun(
            providers: identifyRunOneSource.providers,
            discId: .leftOut(
                discId: "aB7cD9eFgH2iJkL3mN4oP5qR6sT=",
                source: BridgeDiscIdFile(
                    kind: .log,
                    file: "Artist Name - Album Title One.log"
                )
            ),
            barcode: identifyRunOneSource.barcode,
            catalog: identifyRunOneSource.catalog
        )

        /// Two codes on the sleeve and only one of them the disc's: the box
        /// set's is left out, so it stands as an off chip with nothing run
        /// against it while the other is looked up.
        static let identifyRunBarcodeLeftOut = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .absent,
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        sources: [cueBarcodeSource, backCoverBarcodeSource],
                        excluded: false,
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
                        excluded: true,
                        cells: cells(.notAsked, .notAsked)
                    ),
                ]
            ),
            catalog: .numbers(scanning: false, rows: [], candidates: [])
        )

        /// The same two codes with both of them asked about — what the
        /// left-out one is drawn against.
        static let identifyRunBothBarcodesAsked = BridgeIdentifyRun(
            providers: identifyRunBarcodeLeftOut.providers,
            discId: identifyRunBarcodeLeftOut.discId,
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        sources: [cueBarcodeSource, backCoverBarcodeSource],
                        excluded: false,
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
                        excluded: false,
                        cells: cells(.notAsked, .notAsked)
                    ),
                ]
            ),
            catalog: identifyRunBarcodeLeftOut.catalog
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
                        excluded: false,
                        cells: cells(.noMatch, .failed(failure: .timeout))
                    ),
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        sources: [cueBarcodeSource],
                        excluded: false,
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
                        excluded: false,
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
                        excluded: false,
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
                        excluded: false,
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
    }
#endif
