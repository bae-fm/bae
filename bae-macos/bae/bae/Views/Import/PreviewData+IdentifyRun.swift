#if DEBUG
    import BaeKit
    import Foundation

    /// One `BridgeIdentifyRun` per shape the identifier band draws.
    extension PreviewData {
        /// In flight: the disc ID matched, the first barcode is half
        /// answered, and the second is left out.
        static let identifyRunInFlight = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .read(
                discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                lookup: .found(count: 1, groups: [searchGroupExactBridge])
            ),
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        excluded: false,
                        cells: cells(.lookingUp, foundExact)
                    ),
                    BridgeSignalValueRow(
                        value: "9999999999999",
                        excluded: true,
                        cells: cells(leftOut, leftOut)
                    ),
                ]
            ),
            catalog: .numbers(
                scanning: false,
                rows: [],
                candidates: catalogCandidates
            ),
            isrc: .read(
                isrcs: ["XX0000000001", "XX0000000002"],
                lookup: .found(count: 1, groups: [searchGroupExactBridge])
            ),
            search: .notNeeded
        )

        /// Just started, with the artwork still being read.
        static let identifyRunStarting = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .reading,
            barcode: .rows(scanning: true, rows: []),
            catalog: .numbers(scanning: true, rows: [], candidates: []),
            isrc: .absent,
            search: .notNeeded
        )

        /// Discogs alone, which does not answer disc IDs.
        static let identifyRunOneSource = BridgeIdentifyRun(
            providers: [.discogs],
            discId: .read(
                discId: "aB7cD9eFgH2iJkL3mN4oP5qR6sT=",
                lookup: .notAsked(reason: .noCatalog)
            ),
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "5051961234567",
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
            catalog: .numbers(scanning: false, rows: [], candidates: []),
            isrc: .read(
                isrcs: ["XX0000000001"],
                lookup: .notAsked(reason: .noCatalog)
            ),
            search: .notNeeded
        )

        /// The one-source run with its disc ID left out.
        static let identifyRunDiscIdLeftOut = BridgeIdentifyRun(
            providers: identifyRunOneSource.providers,
            discId: .read(
                discId: "aB7cD9eFgH2iJkL3mN4oP5qR6sT=",
                lookup: leftOut
            ),
            barcode: identifyRunOneSource.barcode,
            catalog: identifyRunOneSource.catalog,
            isrc: identifyRunOneSource.isrc,
            search: identifyRunOneSource.search
        )

        /// Two barcodes, one left out.
        static let identifyRunBarcodeLeftOut = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .absent,
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        excluded: false,
                        cells: cells(.lookingUp, foundExact)
                    ),
                    BridgeSignalValueRow(
                        value: "9999999999999",
                        excluded: true,
                        cells: cells(leftOut, leftOut)
                    ),
                ]
            ),
            catalog: .numbers(scanning: false, rows: [], candidates: []),
            isrc: .absent,
            search: .notNeeded
        )

        /// The same two barcodes, both asked about.
        static let identifyRunBothBarcodesAsked = BridgeIdentifyRun(
            providers: identifyRunBarcodeLeftOut.providers,
            discId: identifyRunBarcodeLeftOut.discId,
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        excluded: false,
                        cells: cells(.lookingUp, foundExact)
                    ),
                    BridgeSignalValueRow(
                        value: "9999999999999",
                        excluded: false,
                        cells: cells(.noMatch, .lookingUp)
                    ),
                ]
            ),
            catalog: identifyRunBarcodeLeftOut.catalog,
            isrc: identifyRunBarcodeLeftOut.isrc,
            search: identifyRunBarcodeLeftOut.search
        )

        /// Discogs failed the first barcode while other lookups carry on.
        static let identifyRunProviderFailed = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .absent,
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "5051961234567",
                        excluded: false,
                        cells: cells(.noMatch, .failed(failure: .timeout))
                    ),
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        excluded: false,
                        cells: cells(.lookingUp, .lookingUp)
                    ),
                ]
            ),
            catalog: .numbers(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "LC 6006",
                        excluded: false,
                        cells: cells(.lookingUp, .noMatch)
                    )
                ],
                candidates: Array(catalogCandidates.dropFirst())
            ),
            isrc: .read(
                isrcs: ["XX0000000001"],
                lookup: .failed(failure: .timeout)
            ),
            search: .notNeeded
        )

        /// The provider-failed run with its catalog number taken back out.
        static let identifyRunCatalogWaiting = BridgeIdentifyRun(
            providers: identifyRunProviderFailed.providers,
            discId: identifyRunProviderFailed.discId,
            barcode: identifyRunProviderFailed.barcode,
            catalog: .numbers(
                scanning: false,
                rows: [],
                candidates: catalogCandidates
            ),
            isrc: identifyRunProviderFailed.isrc,
            search: identifyRunProviderFailed.search
        )

        /// Every lookup answered empty.
        static let identifyRunNothingFound = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .read(
                discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                lookup: .noMatch
            ),
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        excluded: false,
                        cells: cells(.noMatch, .noMatch)
                    )
                ]
            ),
            catalog: .numbers(
                scanning: false,
                rows: [],
                candidates: Array(catalogCandidates.prefix(2))
            ),
            isrc: .absent,
            search: .searched(
                album: "Album Title One",
                artist: "Artist Name",
                cells: cells(.noMatch, .noMatch)
            )
        )

        /// Settled, with the disc ID and the barcode both matched.
        static let identifyRunFound = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .read(
                discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                lookup: .found(count: 1, groups: [searchGroupExactBridge])
            ),
            barcode: .rows(
                scanning: false,
                rows: [
                    BridgeSignalValueRow(
                        value: "0123456789012",
                        excluded: false,
                        cells: cells(foundExact, foundExact)
                    )
                ]
            ),
            catalog: .numbers(
                scanning: false,
                rows: [],
                candidates: Array(catalogCandidates.prefix(1))
            ),
            isrc: .absent,
            search: .notNeeded
        )

        /// No disc ID or barcode, only catalog numbers to pick from.
        static let identifyRunAwaitingCatalog = BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .absent,
            barcode: .absent,
            catalog: .numbers(
                scanning: false,
                rows: [],
                candidates: Array(catalogCandidates.prefix(2))
            ),
            isrc: .absent,
            search: .noTitle
        )

        /// A cell nobody was asked because the person left its value out.
        static let leftOut = BridgeLookupState.notAsked(reason: .leftOut)
    }
#endif
