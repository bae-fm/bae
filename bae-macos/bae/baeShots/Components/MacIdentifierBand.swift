import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macIdentifierBandScenes: [ShotScene] = [
        identifierBandScene(
            "mac.component.identifier-band.found",
            PreviewData.identifyRunBandBoard
        ),
        identifierBandScene(
            "mac.component.identifier-band.searching",
            bandRun(.lookingUp)
        ),
        identifierBandScene(
            "mac.component.identifier-band.nothing-found",
            bandRun(.noMatch)
        ),
        identifierBandScene(
            "mac.component.identifier-band.wrapped",
            PreviewData.identifyRunBandBoard,
            width: 300,
            height: 130
        ),
        identifierBandScene(
            "mac.component.identifier-band.left-out",
            leftOutRun
        ),
        identifierBandScene(
            "mac.component.identifier-band.catalog-numbers",
            catalogNumbersRun
        ),
        identifierBandScene(
            "mac.component.identifier-band.catalog-numbers-shown",
            catalogNumbersRun,
            listsCatalogCandidates: true
        ),
        identifierBandScene(
            "mac.component.identifier-band.struck",
            struckRun
        ),
    ]

    /// A catalog number whose release both catalogs name, with the Disc ID
    /// and the barcode left out of the run.
    private static let leftOutRun = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .read(discs: [
            BridgeDiscIdRow(
                discId: "v8n2Kq1Z…",
                lookup: .notAsked(reason: .leftOut)
            )
        ]),
        barcode: .rows(rows: [
            BridgeSignalValueRow(
                value: sampleBarcode,
                excluded: true,
                cells: [.musicBrainz, .discogs]
                    .map {
                        BridgeProviderCell(
                            source: $0,
                            lookup: .notAsked(reason: .leftOut)
                        )
                    }
            )
        ]),
        catalog: .numbers(
            rows: [
                BridgeSignalValueRow(
                    value: "HR-1042",
                    excluded: false,
                    cells: [.musicBrainz, .discogs]
                        .map {
                            BridgeProviderCell(
                                source: $0,
                                lookup: .found(count: 1, groups: [])
                            )
                        }
                )
            ],
            candidates: []
        ),
        isrc: .absent,
        search: .notNeeded,
        readingArtwork: false
    )

    /// A Disc ID that two MusicBrainz releases carry, and a catalog number
    /// the person struck out of the run.
    private static let struckRun = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .read(discs: [
            BridgeDiscIdRow(
                discId: "v8n2Kq1Z…",
                lookup: .found(count: 2, groups: [])
            )
        ]),
        barcode: .rows(rows: []),
        catalog: .numbers(
            rows: [
                BridgeSignalValueRow(
                    value: "HR-1042",
                    excluded: true,
                    cells: []
                )
            ],
            candidates: []
        ),
        isrc: .absent,
        search: .notNeeded,
        readingArtwork: false
    )

    /// No Disc ID, barcode or ISRC, and two catalog numbers that no
    /// release confirms, behind their count.
    private static let catalogNumbersRun = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .absent,
        barcode: .absent,
        catalog: .numbers(
            rows: [],
            candidates: [
                BridgeCatalogCandidate(value: "HR-1042"),
                BridgeCatalogCandidate(value: "HR-1042-2"),
            ]
        ),
        isrc: .absent,
        search: .notNeeded,
        readingArtwork: false
    )

    /// The run where Disc ID and barcode each wait on, or found nothing
    /// from, the catalog that answers them.
    private static func bandRun(
        _ lookup: BridgeLookupState
    ) -> BridgeIdentifyRun {
        BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .read(discs: [
                BridgeDiscIdRow(discId: "v8n2Kq1Z…", lookup: lookup)
            ]),
            barcode: .rows(rows: [
                BridgeSignalValueRow(
                    value: sampleBarcode,
                    excluded: false,
                    cells: barcodeSources(lookup)
                        .map {
                            BridgeProviderCell(source: $0, lookup: lookup)
                        }
                )
            ]),
            catalog: .numbers(rows: [], candidates: []),
            isrc: .absent,
            search: .notNeeded,
            readingArtwork: false
        )
    }

    /// A barcode that nothing matched was asked of both catalogs; one still
    /// being looked up waits on Discogs alone.
    private static func barcodeSources(
        _ lookup: BridgeLookupState
    ) -> [BridgeLookupCatalog] {
        if case .noMatch = lookup { return [.musicBrainz, .discogs] }
        return [.discogs]
    }

    private static func identifierBandScene(
        _ id: String,
        _ run: BridgeIdentifyRun,
        width: CGFloat = 580,
        height: CGFloat = 70,
        listsCatalogCandidates: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: width,
            height: height
        ) {
            MacIdentifierBand(
                run: run,
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
                onEditTitleSearch: { _, _ in },
                listsCatalogCandidates: listsCatalogCandidates
            )
        }
    }
}
