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
            PreviewData.identifyRunBandBoard
        ),
        identifierBandScene(
            "mac.component.identifier-band.struck",
            PreviewData.identifyRunBandBoard
        ),
    ]

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
                    cells: [
                        BridgeProviderCell(source: .discogs, lookup: lookup)
                    ]
                )
            ]),
            catalog: .numbers(rows: [], candidates: []),
            isrc: .absent,
            search: .notNeeded,
            readingArtwork: false
        )
    }

    private static func identifierBandScene(
        _ id: String,
        _ run: BridgeIdentifyRun,
        width: CGFloat = 580,
        height: CGFloat = 70
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
                onEditTitleSearch: { _, _ in }
            )
        }
    }
}
