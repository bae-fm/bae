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
            "mac.component.identifier-band.nothing-found",
            bandRun(.noMatch)
        ),
        identifierBandScene(
            "mac.component.identifier-band.wrapped",
            PreviewData.identifyRunBandBoard,
            width: 300,
            height: 130,
            wrapsAtWidth: true
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
            "mac.component.identifier-band.isrcs",
            isrcsRun
        ),
        isrcsShownScene(
            "mac.component.identifier-band.isrcs-shown"
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

    /// The eleven ISRCs the files' tags carry, as MusicBrainz answered them.
    static let boardIsrcs = (1...11)
        .map {
            String(format: "USHR192001%02d", $0)
        }

    /// The board's found run with the files' ISRCs, which MusicBrainz found
    /// two releases for.
    private static var isrcsRun: BridgeIdentifyRun {
        var run = PreviewData.identifyRunBandBoard
        run.isrc = .read(
            isrcs: boardIsrcs,
            lookup: .found(count: 2, groups: [])
        )
        return run
    }

    /// The ISRC chip's +N pressed: the popover under the button lists every
    /// code, drawn in place as the app draws it, 10 points under the chip.
    private static func isrcsShownScene(_ id: String) -> ShotScene {
        component(id, width: 842, height: 263) {
            ZStack(alignment: .topLeading) {
                MacIdentifierBand(
                    run: isrcsRun,
                    onToggleLookup: { _ in },
                    onToggleCatalogAgreement: { _ in },
                    onRetryFailed: {},
                    onEditTitleSearch: { _, _ in },
                    listsCatalogCandidates: .constant(false)
                )
                moreValuesPopover(
                    boardIsrcs,
                    buttonCenter: isrcsMoreButtonCenter,
                    buttonBottom: 18
                )
            }
            .frame(width: 842, height: 263, alignment: .topLeading)
        }
    }

    /// Where the +N button's centre is from the left of the band, in the
    /// board's found run with ISRCs.
    private static let isrcsMoreButtonCenter: CGFloat = 611.5

    /// A chip's popover of every value, drawn in place as the board draws
    /// it: its right edge 75 points right of the +N button's centre, which is
    /// `buttonCenter` points from the left, its body 10 points under the
    /// button's bottom edge `buttonBottom`, its arrow on the button.
    static func moreValuesPopover(
        _ lines: [String],
        buttonCenter: CGFloat,
        buttonBottom: CGFloat
    ) -> some View {
        let width: CGFloat = 150 + 2 * 14
        return MacPopoverInPlace(arrowSide: .top, arrowOffset: width - 75) {
            MacIdentifierValuesPopover(lines: lines)
        }
        .padding(.leading, buttonCenter + 75 - width - popoverShadowRoom)
        .padding(
            .top,
            buttonBottom + 10 - popoverArrowLength - popoverShadowRoom
        )
    }

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
        wrapsAtWidth: Bool = false,
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
                listsCatalogCandidates: .constant(listsCatalogCandidates)
            )
            // The wrapped state's board holds the band in a narrow box, and
            // every other board lets the band take the width it needs.
            .frame(maxWidth: wrapsAtWidth ? width : nil, alignment: .leading)
        }
    }
}
