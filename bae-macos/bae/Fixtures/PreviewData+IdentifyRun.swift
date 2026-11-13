import BaeKit
import Foundation

@testable import bae

/// One `BridgeIdentifyRun` per shape the identifier band draws.
extension PreviewData {
    /// In flight: the Disc ID matched, the first barcode is half
    /// answered, and the second is left out.
    static let identifyRunInFlight = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .read(discs: [
            BridgeDiscIdRow(
                discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                lookup: .found(count: 1, groups: [searchGroupExactBridge])
            )
        ]),
        barcode: .rows(
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
            rows: [],
            candidates: catalogCandidates
        ),
        isrc: .read(
            isrcs: ["XX0000000001", "XX0000000002"],
            lookup: .found(count: 1, groups: [searchGroupExactBridge])
        ),
        search: .notNeeded,
        readingArtwork: false
    )

    /// Just started, with the artwork still being read.
    static let identifyRunStarting = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .reading,
        barcode: .rows(rows: []),
        catalog: .numbers(rows: [], candidates: []),
        isrc: .absent,
        search: .notNeeded,
        readingArtwork: true
    )

    /// Discogs failed the first barcode while other lookups carry on.
    static let identifyRunProviderFailed = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .absent,
        barcode: .rows(
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
        search: .notNeeded,
        readingArtwork: false
    )

    /// Every lookup answered empty.
    static let identifyRunNothingFound = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .read(discs: [
            BridgeDiscIdRow(
                discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                lookup: .noMatch
            )
        ]),
        barcode: .rows(
            rows: [
                BridgeSignalValueRow(
                    value: "0123456789012",
                    excluded: false,
                    cells: cells(.noMatch, .noMatch)
                )
            ]
        ),
        catalog: .numbers(
            rows: [],
            candidates: Array(catalogCandidates.prefix(2))
        ),
        isrc: .absent,
        search: .searched(
            album: "Album Title One",
            artist: "Artist Name",
            cells: cells(.noMatch, .noMatch)
        ),
        readingArtwork: false
    )

    /// Settled, with the Disc ID and the barcode both matched.
    static let identifyRunFound = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .read(discs: [
            BridgeDiscIdRow(
                discId: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                lookup: .found(count: 1, groups: [searchGroupExactBridge])
            )
        ]),
        barcode: .rows(
            rows: [
                BridgeSignalValueRow(
                    value: "0123456789012",
                    excluded: false,
                    cells: cells(foundExact, foundExact)
                )
            ]
        ),
        catalog: .numbers(
            rows: [],
            candidates: Array(catalogCandidates.prefix(1))
        ),
        isrc: .absent,
        search: .notNeeded,
        readingArtwork: false
    )

    /// No Disc ID or barcode, only catalog numbers to pick from.
    static let identifyRunAwaitingCatalog = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .absent,
        barcode: .absent,
        catalog: .numbers(
            rows: [],
            candidates: Array(catalogCandidates.prefix(2))
        ),
        isrc: .absent,
        search: .noTitle,
        readingArtwork: false
    )

    /// A cell nobody was asked because the person left its value out.
    static let leftOut = BridgeLookupState.notAsked(reason: .leftOut)
}
